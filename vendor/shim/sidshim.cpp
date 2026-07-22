// C ABI shim over libsidplayfp (GPL-2.0-or-later, system library) for the
// Rust SID engine. One handle = one loaded tune + engine + reSIDfp builder.
//
// libsidplayfp 2.14+ flow: engine.play(cycles) emulates and reports how many
// samples each SID produced; engine.mix(buf, n) drains them interleaved
// (stereo after initMixer(true)). We drain fully into a carry buffer so no
// produced samples are ever lost between render calls.

#include <sidplayfp/sidplayfp.h>
#include <sidplayfp/SidConfig.h>
#include <sidplayfp/SidInfo.h>
#include <sidplayfp/SidTune.h>
#include <sidplayfp/SidTuneInfo.h>
#include <sidplayfp/SidDatabase.h>
#include <sidplayfp/builders/residfp.h>

#include <cstdint>
#include <cstring>
#include <exception>
#include <string>
#include <vector>

namespace {

struct Shim {
    // Declaration order is load-bearing: members are destroyed in reverse, and
    // libsidplayfp requires the builder (and tune) to outlive the engine, which
    // releases its SID emulations back to the builder on teardown. `engine` is
    // therefore declared LAST so it is destroyed FIRST.
    ReSIDfpBuilder builder{"minerva-residfp"};
    SidTune tune;
    std::string error;
    std::vector<int16_t> carry; // mixed stereo samples not yet handed out
    size_t carry_pos = 0;       // in i16 units
    sidplayfp engine;

    Shim(const uint8_t* data, size_t len) : tune(data, (uint_least32_t)len) {}
};

thread_local std::string g_create_error;

} // namespace

extern "C" {

// song: 1-based subtune, 0 = the tune's default. Returns null on failure and
// sets *err_out (valid until the next create on this thread).
void* sidshim_create(const uint8_t* data, size_t len, unsigned int song,
                     uint32_t sample_rate, const char** err_out) try {
    Shim* s = new Shim(data, len);
    const char* fail = nullptr;

    if (!s->tune.getStatus()) {
        fail = s->tune.statusString();
    } else {
        s->tune.selectSong(song);
        // The engine instantiates the reSIDfp emus itself during config().
        SidConfig cfg = s->engine.config();
        cfg.frequency = sample_rate;
        cfg.sidEmulation = &s->builder;
        if (!s->engine.config(cfg)) {
            fail = s->engine.error();
        } else if (!s->engine.load(&s->tune)) {
            fail = s->engine.error();
        } else {
            s->engine.initMixer(true);
        }
    }

    if (fail) {
        g_create_error = fail;
        if (err_out) *err_out = g_create_error.c_str();
        delete s;
        return nullptr;
    }
    return s;
} catch (const std::exception& e) {
    // No exception may cross the extern "C" boundary into Rust (UB).
    g_create_error = e.what();
    if (err_out) *err_out = g_create_error.c_str();
    return nullptr;
} catch (...) {
    g_create_error = "unknown C++ exception in sidshim_create";
    if (err_out) *err_out = g_create_error.c_str();
    return nullptr;
}

// Fill `frames` interleaved stereo frames. Returns frames written, < 0 on
// engine error (message via sidshim_error).
int sidshim_render(void* h, int16_t* out, int frames) {
    Shim* s = static_cast<Shim*>(h);
    try {
    int filled = 0;
    int idle_rounds = 0;
    while (filled < frames) {
        size_t avail = s->carry.size() - s->carry_pos;
        if (avail > 0) {
            size_t want = (size_t)(frames - filled) * 2;
            size_t take = avail < want ? avail : want;
            std::memcpy(out + (size_t)filled * 2, s->carry.data() + s->carry_pos,
                        take * sizeof(int16_t));
            s->carry_pos += take;
            filled += (int)(take / 2);
            continue;
        }
        // ~20k C64 cycles ≈ 20 ms ≈ 900 samples at 44.1 kHz.
        int produced = s->engine.play(20000);
        if (produced < 0) {
            s->error = s->engine.error();
            return -1;
        }
        if (produced == 0) {
            if (++idle_rounds > 8) break; // defensive: never spin forever
            continue;
        }
        idle_rounds = 0;
        s->carry.assign((size_t)produced * 2, 0);
        s->carry_pos = 0;
        unsigned mixed = s->engine.mix(s->carry.data(), (unsigned)produced);
        s->carry.resize(mixed); // stereo: mix returns total i16 count
    }
    return filled;
    } catch (const std::exception& e) {
        s->error = e.what();
        return -1;
    } catch (...) {
        s->error = "unknown C++ exception in sidshim_render";
        return -1;
    }
}

const char* sidshim_error(void* h) {
    return static_cast<Shim*>(h)->error.c_str();
}

unsigned sidshim_songs(void* h) {
    return static_cast<Shim*>(h)->tune.getInfo()->songs();
}

unsigned sidshim_selected_song(void* h) {
    return static_cast<Shim*>(h)->tune.getInfo()->currentSong();
}

// idx: 0 = title, 1 = author, 2 = released. Pointer owned by the tune.
const char* sidshim_info(void* h, unsigned idx) {
    const SidTuneInfo* info = static_cast<Shim*>(h)->tune.getInfo();
    return idx < info->numberOfInfoStrings() ? info->infoString(idx) : "";
}

void sidshim_destroy(void* h) {
    delete static_cast<Shim*>(h);
}

// ---- Songlengths database (HVSC Songlengths.md5) ------------------------

void* sidshim_db_open(const char* path) try {
    SidDatabase* db = new SidDatabase();
    if (!db->open(path)) {
        delete db;
        return nullptr;
    }
    return db;
} catch (...) {
    return nullptr;
}

// Duration of `h`'s selected subtune in milliseconds, -1 if unknown.
int32_t sidshim_db_length_ms(void* db, void* h) try {
    return static_cast<SidDatabase*>(db)->lengthMs(static_cast<Shim*>(h)->tune);
} catch (...) {
    return -1;
}

void sidshim_db_close(void* db) {
    delete static_cast<SidDatabase*>(db);
}

} // extern "C"
