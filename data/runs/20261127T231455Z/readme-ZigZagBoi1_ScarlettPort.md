<div align="center">

# ScarlettPort

**Run your own Xbox Series X|S dump on a Windows PC. Its own runtime, its own launcher.**

<a href="https://github.com/ZigZagBoi1/ScarlettPort"><img src="https://img.shields.io/github/stars/ZigZagBoi1/ScarlettPort?style=flat&logo=github&label=stars&color=ffd700" alt="GitHub stars"></a>
<a href="https://github.com/ZigZagBoi1/ScarlettPort/releases/latest"><img src="https://img.shields.io/github/v/release/ZigZagBoi1/ScarlettPort?style=flat&label=release" alt="Latest release"></a>
<a href="https://github.com/ZigZagBoi1/ScarlettPort/releases"><img src="https://img.shields.io/github/downloads/ZigZagBoi1/ScarlettPort/total?style=flat&label=downloads" alt="Downloads"></a>
<a href="LICENSE.md"><img src="https://img.shields.io/badge/licence-GPL-blue?style=flat" alt="Licence"></a>

<b>⭐ If ScarlettPort helps you, please star the repo, it helps more people find it.</b>

> *"Almost everything you download was made by a company. This was made by a person who could not stop wondering whether it was possible. If it gave you even one moment of that same wonder, a single dollar is how you tell someone their late nights mattered."*
>
> Ziggy

<a href="https://www.paypal.me/SalwanStudios"><img src="https://img.shields.io/badge/DONATE-PayPal-blue?style=for-the-badge&logo=paypal&logoColor=white" alt="Donate via PayPal"></a>

<sub>Donations are voluntary gifts that support development time. They are not a purchase: they buy no game content, access, builds or perks. PayPal: please send as <b>Friends & Family</b> (a gift).</sub>

</div>

<img src="assets/cell-v050.png" width="100%" alt="Opening cell drawn by the ScarlettPort runtime">

<sub>ScarlettPort v0.5.0 on an RTX 3060 (Series S profile, ray tracing off).</sub>

## What it is

A bring-your-own-copy compatibility layer, in the spirit of Wine and Proton. No emulator, no streaming: the title's own x64 code runs on your CPU, and its RDNA2 GPU code is recompiled for your graphics card on the fly.

You supply a dump of your own legally purchased copy. Nothing of any title is included here.

## Status

![progress](https://img.shields.io/badge/progress-90%25-ff2d95?style=flat)

```
██████████████████░░  90%
```

**Done:** boots, gets through menus, saves and legal screens, reaches the world (in 178 s in 0.5.0) and gives you control in the opening area, with the full picture: lit rooms, ambient and sky light, complete textures.

**Remaining:**

- **Frame rate:** about 7-8 fps in the world on an RTX 3060 12 GB + Ryzen 5 5600X with a warm cache (v0.5.2, Series S profile). The goal is a playable 30 fps.
- **Stability over long sessions:** no long run has been recorded yet.

**New in 0.5.5** (fixes merged after 0.5.4; built and unit-tested):
- Laptops with integrated and dedicated graphics: the engine now runs on the high-performance GPU and logs its name, instead of the default adapter, which is the integrated one ([#292](https://github.com/ZigZagBoi1/ScarlettPort/pull/292), [#274](https://github.com/ZigZagBoi1/ScarlettPort/issues/274)).
- The launcher finds FSR from the game's own AMD upscaler DLL in the game folder ([#164](https://github.com/ZigZagBoi1/ScarlettPort/pull/164), [#160](https://github.com/ZigZagBoi1/ScarlettPort/issues/160)).
- Ported from trustno1-coder/VIctory-NVIDIA with credit: console shader lanes spread over several host waves take their threads from a group counter (`SCARLETT_THREAD_TICKET=0` restores the old numbering), the violet patches on lit paper are gone, dynamic descriptor slots go back only to their own heap; opt-in `SCARLETT_HELPER_PRIORITY`, `SCARLETT_RAW_VIEW_CLAMP` and `SCARLETT_SKIP_BINK` ([#304](https://github.com/ZigZagBoi1/ScarlettPort/pull/304)).
- Without ray tracing, the translucent back-light is cleared after the final gather (`SCARLETT_TRANSLUCENT_BACKLIGHT=1` keeps the old look) ([#222](https://github.com/ZigZagBoi1/ScarlettPort/pull/222)).
- `scarlett pipeline-cache` reads the game's own precompiled pipeline cache from your files and can seed and fill the pipeline library ahead of a first run (`--seed`, `--fill`; a command for now, not yet part of Prepare) ([#223](https://github.com/ZigZagBoi1/ScarlettPort/pull/223), [#249](https://github.com/ZigZagBoi1/ScarlettPort/pull/249)-[#254](https://github.com/ZigZagBoi1/ScarlettPort/pull/254)).
- Shader warm-up stops scanning at its deadline ([#200](https://github.com/ZigZagBoi1/ScarlettPort/pull/200)).

**New in 0.5.4** (fixes merged after 0.5.3; built and unit-tested):
- Black skin on AMD: a pipeline listed as bad after one crash while warming is built again on its own at every Prepare and taken off the list when it builds, so no pipeline stays skipped for good ([#146](https://github.com/ZigZagBoi1/ScarlettPort/pull/146), [#92](https://github.com/ZigZagBoi1/ScarlettPort/issues/92)).
- The launcher minimizes while the game runs and comes back when it closes or crashes, then offers to send a crash report (a prefilled GitHub issue you check before submitting) ([#141](https://github.com/ZigZagBoi1/ScarlettPort/pull/141)).
- The launcher's main tab no longer says shaders are ready when the cache isn't prepared for the current driver and preset ([#141](https://github.com/ZigZagBoi1/ScarlettPort/pull/141), [#135](https://github.com/ZigZagBoi1/ScarlettPort/issues/135)).
- A damaged game file is named in the log and the crash log, with how to check it ([#127](https://github.com/ZigZagBoi1/ScarlettPort/pull/127)).
- Game folders behind symbolic links are found, shader scans are faster, split-storage verify fixed ([#134](https://github.com/ZigZagBoi1/ScarlettPort/pull/134)); a CLI-built shader cache is reused only when its translator matches ([#153](https://github.com/ZigZagBoi1/ScarlettPort/pull/153)).
- Read-only CLI commands no longer move old data folders ([#131](https://github.com/ZigZagBoi1/ScarlettPort/pull/131)); embedded shader containers keep their hashes ([#114](https://github.com/ZigZagBoi1/ScarlettPort/pull/114)); a misleading "pipeline missing" warning removed ([#147](https://github.com/ZigZagBoi1/ScarlettPort/pull/147)).

**New in 0.5.3** (fixes merged after 0.5.2; built and unit-tested, the AMD fixes not checked on those cards by us):
- RX 6000 (RDNA 2): mesh pipelines are made one at a time on AMD and the mapped pipeline library is kept ([#88](https://github.com/ZigZagBoi1/ScarlettPort/pull/88)).
- RX 9000 (RDNA 4): a raw view's size is read through its own handle, plus the newer device ids ([#93](https://github.com/ZigZagBoi1/ScarlettPort/pull/93)).
- AMD driver crashes while warming shaders: a pipeline is retried alone before it is listed as bad ([#104](https://github.com/ZigZagBoi1/ScarlettPort/pull/104)).
- BC7 textures: the channel rotation is read only in modes 4 and 5, which swapped alpha into a colour channel in some textures ([#108](https://github.com/ZigZagBoi1/ScarlettPort/pull/108)).
- Indirect draws whose arguments were written late are clamped one by one ([#101](https://github.com/ZigZagBoi1/ScarlettPort/pull/101)); every refused pipeline logs its reason.
- NVIDIA frame-time work from Belobug and avatarmanz, fewer heap re-sets; F11 toggles borderless fullscreen ([#95](https://github.com/ZigZagBoi1/ScarlettPort/pull/95)).
- Launcher 0.5.3: names the discrete GPU in hardware reports and shows the right bundled runtime version.

**New in 0.5.2** (built and unit-tested, not yet measured in a game run on our RTX 3060):
- No more black screen after loading on the Stable preset: the engine uses the game's own upscaler and falls back to it when the AMD one is missing ([#60](https://github.com/ZigZagBoi1/ScarlettPort/pull/60), [#61](https://github.com/ZigZagBoi1/ScarlettPort/issues/61)).
- No 500 ms wait per frame for pipelines still building on first runs.
- Levels on demand finally take effect: 18.9 to 22.0 fps median on an RX 9060 XT ([#64](https://github.com/ZigZagBoi1/ScarlettPort/pull/64)).
- 40 shader variants restored, integer-target blending repaired, the voxel visibility shader recovered (from DexNom/VIctory); a shader size query no longer reused, which crashed AMD's compiler on 29 compute pipelines (from Belobug/VIctory-Belobug-AMD).
- Mouse and keyboard: opt-in PC-style layout with mouse look (`SCARLETT_KBM=1`); DualSense and DualShock 4 pads read directly.
- Clear out-of-memory message in the log ([#54](https://github.com/ZigZagBoi1/ScarlettPort/pull/54)), more GPU detail in player logs, shader progress every 100 shaders.
- Launcher 0.5.2: live shader progress that reaches 100%, two cores left free for the desktop, an update banner with one-click Update now.

**New in 0.5.0** (RTX 3060 12 GB, Ryzen 5 5600X, 32 GB, one boot each, 0.4.x defaults against 0.5.0 defaults):

- The save emulation no longer sleeps retrying old save folders it cannot delete (it slept about 243 s per boot).
- The upscaler's 24 MB network buffer is kept in card memory: its slowest pass takes 0.10-0.28 ms per frame instead of 55.7-70.8 ms.
- Two shader translation fixes are on by default (the first run rebuilds the shader cache once).
- World reached in 178 s instead of 244 s (one boot each; load times vary from boot to boot), and 3.4 fps in the world instead of 1.6. The 95th-percentile frame did not improve (4,464 ms against 4,250 ms).
- Load work (read-ahead, unpaced loading frames, `scarlett prepare`) is opt-in in the Experimental preset; not measured on its own yet.
- Launcher 0.5.0: a first-run setup (game folder search, player name, one-time shader preparation).
- Launcher 0.5.1: a calmer game check (the dump's own file list is advisory), a Setup page with every step's status and a way back into setup, mouse look with sensitivity, the game below normal priority so the PC stays responsive, a shader bar that reaches 100%, scrolling pages, and one-click Update now.

**Known issues:** haze indoors; NVIDIA cards are still slow (3-6 fps); on 16 GB RAM, set a larger Windows paging file ([#51](https://github.com/ZigZagBoi1/ScarlettPort/issues/51)).

### Community results

Hardware reports from testers. These are **their own measurements**, not ours.

- **RX 9060 XT 16 GB + Ryzen 7 7700X**, 0.5.2 (levels on demand fixed): 22.0 fps median (45.4 ms), up from 18.9. [#64](https://github.com/ZigZagBoi1/ScarlettPort/pull/64)
- **RX 9060 XT 16 GB + Ryzen 7 7700X**, v0.4.9 after warm: 18.7 fps median (53.4 ms; best 32.9 ms), up 3.3x from v0.4.4. [#53](https://github.com/ZigZagBoi1/ScarlettPort/issues/53)
- **RTX 3070 8 GB + Ryzen 7 5700X**, v0.5.2: 1 h 20 min without a crash, 8.5 fps in the world, 30 in menus (warm cache). [#106](https://github.com/ZigZagBoi1/ScarlettPort/issues/106)
- **RTX 3060 + Core i5-11400**, v0.5.2: 9.1 fps average (2.9-30). [#109](https://github.com/ZigZagBoi1/ScarlettPort/issues/109)
- **RTX 5060 8 GB + Core i5-14400F**, v0.5.2: up to 15 fps in the prologue, about 8 in the police scene. [#107](https://github.com/ZigZagBoi1/ScarlettPort/issues/107)
- **RTX 5070 Ti + Ryzen 5 7600X**, v0.5.3: 12.8 fps average in the jail scene. [#138](https://github.com/ZigZagBoi1/ScarlettPort/issues/138)
- **RTX 3070 8 GB**, v0.5.3: 9.3 fps world median, up from 8.3 on v0.5.2. [#118](https://github.com/ZigZagBoi1/ScarlettPort/issues/118)
- **RTX 4070 Ti + Core i5-13600KF**, v0.5.1: 13.4 fps average (3.3-30.1). [#76](https://github.com/ZigZagBoi1/ScarlettPort/issues/76)
- **RTX 4060 Laptop 8 GB + Core i9-13980HX**, v0.5.1: 5-7 fps in the cell, 3-4 walking (215 ms of 275 on the GPU). [#68](https://github.com/ZigZagBoi1/ScarlettPort/issues/68)
- **RTX 4070 12 GB + Ryzen 9 3900X**, v0.5.0: whole prologue played, 3-6 fps in the world, 30 fps in menus. [#56](https://github.com/ZigZagBoi1/ScarlettPort/issues/56)
- **RTX 5070 12 GB + Ryzen 7 5800X3D**, v0.4.6: 7-8 fps. [#33](https://github.com/ZigZagBoi1/ScarlettPort/issues/33)
- **RX 9060 XT**, v0.4.4: 5.3-6.0 fps. [#43](https://github.com/ZigZagBoi1/ScarlettPort/issues/43)
- **RTX 3080 10 GB:** 3-4 fps in the world, through the prologue into the first mission. [#26](https://github.com/ZigZagBoi1/ScarlettPort/issues/26)
- **RTX 3060 12 GB**, v0.4.5: about 3 fps. [#47](https://github.com/ZigZagBoi1/ScarlettPort/issues/47)
- **Ours, RTX 3060 12 GB + Ryzen 5 5600X + 32 GB**, v0.5.0: 3.4 fps in the world (1.6 fps with the 0.4.x defaults in the same test).

<details>
<summary><b>Community results table</b></summary>

| Report | GPU | CPU | Version | Their result |
|---|---|---|---|---|
| [#64](https://github.com/ZigZagBoi1/ScarlettPort/pull/64) | RX 9060 XT 16 GB | Ryzen 7 7700X | 0.5.2 | 22.0 fps median (45.4 ms) |
| [#53](https://github.com/ZigZagBoi1/ScarlettPort/issues/53) | RX 9060 XT 16 GB | Ryzen 7 7700X | 0.4.9, after warm | 18.7 fps median (53.4 ms; best 32.9 ms), 3.3x over 0.4.4 |
| [#106](https://github.com/ZigZagBoi1/ScarlettPort/issues/106) | RTX 3070 8 GB | Ryzen 7 5700X | 0.5.2 | 8.5 fps in the world, 80 min without a crash |
| [#109](https://github.com/ZigZagBoi1/ScarlettPort/issues/109) | RTX 3060 | Core i5-11400 | 0.5.2 | 9.1 fps average |
| [#107](https://github.com/ZigZagBoi1/ScarlettPort/issues/107) | RTX 5060 8 GB | Core i5-14400F | 0.5.2 | up to 15 fps in the prologue |
| [#138](https://github.com/ZigZagBoi1/ScarlettPort/issues/138) | RTX 5070 Ti | Ryzen 5 7600X | 0.5.3 | 12.8 fps average in the jail scene |
| [#118](https://github.com/ZigZagBoi1/ScarlettPort/issues/118) | RTX 3070 8 GB | Ryzen 7 5700X | 0.5.3 | 9.3 fps world median (8.3 on 0.5.2), 30 in menus |
| [#137](https://github.com/ZigZagBoi1/ScarlettPort/issues/137) | RTX 3080 10 GB | Windows 10 | 0.5.3 | about 7 fps in the world, 30 in menus |
| [#117](https://github.com/ZigZagBoi1/ScarlettPort/issues/117) | RTX 4070 12 GB |  | 0.5.2 | 5-7 fps through the prologue |
| [#76](https://github.com/ZigZagBoi1/ScarlettPort/issues/76) | RTX 4070 Ti | Core i5-13600KF | 0.5.1 | 13.4 fps average (3.3-30.1) |
| [#68](https://github.com/ZigZagBoi1/ScarlettPort/issues/68) | RTX 4060 Laptop 8 GB | Core i9-13980HX | 0.5.1 | 5-7 fps in the cell, 3-4 walking |
| [#56](https://github.com/ZigZagBoi1/ScarlettPort/issues/56) | RTX 4070 12 GB | Ryzen 9 3900X | 0.5.0 | 3-6 fps in the world, whole prologue |
| [#33](https://github.com/ZigZagBoi1/ScarlettPort/issues/33) | RTX 5070 12 GB | Ryzen 7 5800X3D | 0.4.6 | 7-8 fps |
| [#43](https://github.com/ZigZagBoi1/ScarlettPort/issues/43) | RX 9060 XT | Ryzen 7 7700X | 0.4.4 | 5.3-6.0 fps |
| [#26](https://github.com/ZigZagBoi1/ScarlettPort/issues/26) | RTX 3080 10 GB | Ryzen 9 3900X | | 3-4 fps in the world, prologue into the first mission |
| [#47](https://github.com/ZigZagBoi1/ScarlettPort/issues/47) | RTX 3060 12 GB | Ryzen 5 5600X | 0.4.5 | about 3 fps |

</details>

## Install

1. Download `ScarlettPort-<version>-win64.zip` from [Releases](https://github.com/ZigZagBoi1/ScarlettPort/releases).
2. Right-click the ZIP, **Extract All** (keep `ScarlettPort.exe` and the `runtime` folder side by side).
3. Run `ScarlettPort.exe`.
4. Pick your game folder. It prepares shaders once, then press **Play**.

**Requirements**

- Windows 10/11 x64
- DirectX 12 GPU with Shader Model 6.6 and mesh shaders (RTX 20-series / RX 6000 or newer)
- 32 GB RAM recommended
- Your own game files (your own dump of your own copy)

<sub>The release is built by CI from this repo, is not code-signed, and lists SHA-256 checksums in its notes.</sub>

## More

<details>
<summary><b>Progress and task board</b></summary>

`boots` ✅ → `intro & menus` ✅ → `world loads` ✅ → `full picture` ✅ → `gameplay starts` ✅ → `playable speed` ⏳

| Stage | Status | Notes |
|---|:---:|---|
| Executable loads, imports bind | ✅ | the title's console DLLs load |
| Intro, setup and menus | ✅ | 30 fps, audio plays |
| Saves and legal screens | ✅ | the title saves a checkpoint and loads it again |
| Open world loads | ✅ | world data streams in through DirectStorage |
| Lit world renders | ✅ | since 0.4.6 the full picture (lit rooms, ambient and sky light, complete textures) at about 3 fps on an RTX 3060 (Series S profile); the older darker picture ran at about 5.4 fps |
| Gameplay starts | ✅ | the prologue runs; not played end to end |
| Pipelines prebuilt | ✅ | 25,200 of 25,471 graphics pipelines made ahead of time, and used in play |
| Shaders translate | ✅ | all 26,084 translate; 26,013 pass DXIL validation on their own, the other 71 (oversized mesh shaders) validate only when linked with their pixel shader. XCL's counts differ ([`docs/STATUS.md`](docs/STATUS.md)) |
| Keyboard and mouse | ✅ | click to capture, W walks the character |
| Tile-grid overlay over gameplay | ✅ | fixed (rectangle-list pipelines decode right) |
| Shadowed areas | ✅ | lit by the 0.4.6 renderer's ambient and sky light |
| Intro cutscene | 🔨 | very slow unless skipped |
| Speed work | 🔨 | stable frame pacing, yard stutter fixed, copies cut from 286 to 84 MB per frame; pipeline compile freezes on first runs |
| Playable | ⏳ | not verified |
| 30 fps on a mid-range PC | ⏳ | the goal |

### Still in progress

"In testing" means built and unit-tested, waiting for A/B game runs on our RTX 3060; "planned" means not built yet. Measured so far: the texture queue work in 0.4.3 (5.4 fps, 3.9-4.5 before); the other items have no measured fps gain yet.

| Work | Status | What it does |
|---|:---:|---|
| Missing geometry: character skin shaders | 🧪 in testing | skin, hair and clothes pixel shaders that were refused (draws dropped) translate again |
| Missing geometry: mesh output limit | 🧪 in testing | the 71 over-budget mesh shaders are linked under D3D12's 32 KiB output limit instead of being dropped on NVIDIA |
| Missing geometry: view heap | 🧪 in testing | view heap sized so world draws no longer run out of slots on NVIDIA |
| CPU recording cuts | 🧪 in testing | less per-draw preparing and recording cost on the CPU, with a per-part timing line to measure it |
| Zero-copy title memory | 🔨 in progress | `SCARLETT_ZERO_COPY`: the GPU reads title memory without copies; needs placing to work at scale |
| Shader optimizer | 🧪 in testing | the safe mask of `SCARLETT_SHADER_OPT` removes 51% of instructions offline; the in-game test is pending |
| Hair too bright | ✅ done | hair and lighting render correctly in the opening cell (0.4.3) |
| Texture upload queue | ✅ done | fair order (`SCARLETT_FAIR_DUE=2`, default): about 110 textures waiting at the end of a run, against 21,000 or more before |
| Ray-tracing BVH builder | 🔨 in progress | `SCARLETT_BVH_BUILDER` still makes stray writes |
| Load time | 🔨 in progress | 0.5.0 reaches the world in 178 s (244 s with the 0.4.x defaults, one boot each, RTX 3060); read-ahead and unpaced loading frames are opt-in (Experimental), not measured alone; a load cache (`SCARLETT_LOAD_CACHE`) and a faster decoder (`SCARLETT_FAST_INFLATE`) are in the runtime, off by default, not measured yet |
| World-entry copy stall | 📋 planned | cut the copies that stall entering the world |
| PC responsiveness | 🧪 in testing | `SCARLETT_FRIENDLY=1` (set by the Stable preset): background work leaves processor time to the desktop |
| Textures never uploaded | 📋 planned | 14,830 textures are never uploaded |
| FSR upscaler | 📋 planned | the title's FidelityFX calls answered by AMD's PC FidelityFX |
| Frame generation | 📋 planned | only once the base frame rate is high enough for it to make sense |
| Save safety | 🧪 in testing | protects your save folder from being overwritten by test runs |
| One-command benchmark queue | 🧪 in testing | queues A/B runs of switch sets and collects the frame-time logs in one command |
| Launcher: Controller line | 🧪 in testing | the Play page shows a connected Xbox pad or PlayStation pad (DualShock 4, DualSense, Edge); PlayStation pads are read natively over USB (`SCARLETT_PAD_PS`) |
| Launcher: Prepare shaders | 🧪 in testing | `scarlett shaders` translates every shader, then `scarlett warm` makes the pipelines earlier runs recorded at low priority, with progress and Stop/resume, so moving in the world does not stutter on first use |
| Launcher: drive note | 🧪 in testing | Settings says to keep the game folder on an internal NVMe/SSD for load times |

### Task board: grab one, send a PR

Every task is sized for **one contributor**. Pick one, comment on its issue to claim it, and send a PR. Read [`CONTRIBUTING.md`](CONTRIBUTING.md) first (it takes two minutes).

**Rendering**

- [ ] **#1 Light in shadow.** With ray tracing enabled (off by default), the opening cell has bounce light on an RX 7900 XTX. Other interiors and GPUs still need testing. See `engine/docs/fork-ray-tracing.md`.
- [ ] **#2 Missing walls and roofs.** Some cards lose the first room's walls and roof with certain switches; find the cause (see `engine/VICKY.md`).
- [ ] **#3 Faster wave64 on NVIDIA.** 64-lane programs run as two 32-lane halves. Profile them and cut the overhead.
- [ ] **#9 `SCARLETT_SHADER_OPT` in game.** The safe mask removes 51% of instructions offline; its in-game test is pending. One of the other clean-up passes loses walls in game, and an offline differential found nothing. Run the safe mask in game, then find which pass loses walls and why. `engine/crates/scarlett-shader/src/emit/mod.rs` (`shader_opt`), `emit/bitcode.rs`.
- [x] **#10 Hair renders too bright.** Done in 0.4.3: hair and lighting render correctly in the opening cell. Reports from other scenes are welcome. `engine/crates/scarlett-gpu/src/pipeline/make.rs`.
- [ ] **#11 Ray-tracing BVH builder.** `SCARLETT_BVH_BUILDER` makes stray writes; find and fix them (upstream issues #2402 and #916 are related, see `CREDITS.md`). `engine/crates/scarlett-gpu/src/pipeline.rs`, `engine/crates/scarlett-shader/src/bvh.rs`.
- [ ] **#12 Textures never uploaded.** 0.4.3 makes the upload queue fair (`SCARLETT_FAIR_DUE=2`, default): about 110 textures are waiting at the end of a run, against 21,000 or more before. Check what is still left waiting and why. `engine/crates/scarlett-gpu/src/texture.rs`, `descriptor/host.rs`.

**Performance**

- [ ] **#4 Frame time.** 0.5.0 runs the full picture at 3.4 fps in the world on the RTX 3060 (median frame 664 ms, 95th percentile 4,464 ms); the goal is 30 fps. Earlier: the 0.4.6 full picture ran at about 3 fps on the RTX 3060 (7-8 fps on an RTX 5070). On the older, darker picture the thread that gates each present spends about 187 ms per frame: about 55 ms preparing, 31 ms in refresh copies, 76 ms waiting on the card after title writes next to shader-written memory, 19 ms waiting for title reads. An infinitely fast card would still leave about 99 ms, so the waits and copies are the work to do. See `engine/crates/scarlett-gpu/src/host.rs`, `draw.rs`.
- [ ] **#5 Intro cutscene speed.** The intro is very slow unless skipped; find out why.
- [ ] **#13 Zero-copy title memory.** `SCARLETT_ZERO_COPY` maps title memory for the GPU without copies; it needs placing to work at scale. `engine/crates/scarlett-gpu/src/zerocopy.rs`, `engine/crates/scarlett-hle/src/modules/xmem.rs`.
- [ ] **#14 Load time.** 0.5.0 reaches the world in 178 s (244 s before, one boot each). Measure `SCARLETT_PREFETCH` and `SCARLETT_LOAD_UNPACED` on their own. A load cache (`SCARLETT_LOAD_CACHE`) and a faster decoder (`SCARLETT_FAST_INFLATE`) are in the runtime, off by default: measure them. `engine/crates/scarlett-hle/src/modules/dstorage/loadcache.rs`.
- [ ] **#15 World-entry copy stall.** Entering the world stalls on memory copies; find what is copied and cut it. `engine/crates/scarlett-gpu/src/memcopy.rs`, `host.rs`.
- [x] **#16 PC responsiveness while running.** Done: `SCARLETT_FRIENDLY=1` keeps the rest of the PC usable (thread priorities, worker counts); the Stable preset sets it. `engine/crates/scarlett-core/src/friendly.rs`.
- [x] **#6 Shader cache warm-up.** Done: `scarlett precompile-graphics` and the shader prewarm (`engine/apps/scarlett/src/translate.rs`, `engine/crates/scarlett-gpu/src/pipeline/precompiled.rs`).

- [ ] **#51 Crash mid-prologue.** A crash was reported partway through the prologue; reproduce it and attach the crash report.
- [ ] **Long sessions.** No long run has been recorded yet: play for an hour and report crashes, memory use and fps.
- [ ] **Indoor haze.** Rooms look hazy indoors; find the pass that causes it.

**Tooling and docs**

- [ ] **#7 Hardware reports.** Run it on your GPU and post results (GPU, driver, fps, how far it got).
- [ ] **Look preset A/B.** New in 0.4.4 and not yet measured: run Stable and Look (`SCARLETT_LOOK=1`, `SCARLETT_SHADER_FIXES=1`) in the same scene and report fps and what the picture gains or loses.
- [x] **#8 One-click launcher** for the ScarlettPort runtime (done: it is the launcher's default engine).
- [x] **Launcher 0.4.1.** Done: Controller line on the Play page, the Prepare shaders step, the NVMe/SSD note, and the Stable preset matched to the measured best (native PlayStation pads, no lazy remap).
- [ ] **Controller hot-plug report.** Try the Controller line with wireless (Bluetooth) PlayStation pads and other Xbox-compatible pads, and report what it shows. `launcher/src/controller.rs`.
- [x] **Bring `SCARLETT_FRIENDLY` to the vendored runtime.** Done in 0.4.2: the runtime has it and the Stable preset sets it. `launcher/src/presets.rs`.

Details: [`docs/STATUS.md`](docs/STATUS.md). Plan: [`docs/ROADMAP.md`](docs/ROADMAP.md).

</details>

<details>
<summary><b>Measured results</b></summary>

Measured on an RTX 3060 12 GB with a Ryzen 5 5600X (Series S profile, the title's own upscaler).

- **0.5.0:** world reached in 178 s (244 s with the 0.4.x defaults); 3.4 fps in the world (1.6), median frame 664 ms (821), 95th percentile 4,464 ms (4,250); the upscaler's three network passes took 55.7-70.8 / 43.7-50.0 / 34.7-40.1 ms per frame before, 0.10-0.28 ms and below 0.1 ms after. One boot each, no crashes.
- **Full picture (0.4.6):** about 3 fps in the world. A hardware report measured 7-8 fps on an RTX 5070.
- **Older, darker picture (0.4.3):** 5.4 fps while walking and turning (median frame 184 ms, 95th percentile 197 ms, 99th percentile 207 ms; 3.9-4.5 fps before). 30 fps is not reached.
- **Where the frame goes** (thread that gates each present): about 187 ms per frame, of which about 55 ms is preparing draws, about 31 ms is refresh copies, about 76 ms is waiting on the card after the title writes next to memory that shaders wrote, and about 19 ms is waiting for title reads.
- An infinitely fast card would still leave about 99 ms per frame; free copy-back would leave about 180 ms; a quarter of the resolution saves only about 13 ms of GPU time. So the card is not the limit: the waits and the processor work around it are.
- **GPU profiler** (`SCARLETT_GPU_PROFILE`): about 93-111 ms of GPU time per frame, spread over about 1,100 pipelines, so no single pass is the one to fix.
- **Tested with no gain:** the shader optimizer mask, the pipelining switch, a lock split, thin LTO and removing the settle.
- **Texture queue:** about 110 textures left waiting at the end of a run, against 21,000 or more before.
- **Copies:** cut from 286 to 84 MB per frame.
- **Load time:** the world is reached in about 212 s from an internal SSD, against 300 s or more before.
- **Shaders:** all 26,084 programs translate; 25,200 of 25,471 graphics pipelines are made ahead of play.

### What blocks a good picture

- **Speed:** see above. The intro cutscene is very slow unless it is skipped.
- **Pipeline compile freezes:** on a first run, moving into new areas freezes while pipelines compile. The shader prewarm and the pipeline cache shorten this on later runs.
- **Shadowed areas are too dark.** Hair and lighting render correctly in the opening cell. Ray tracing is off by default. With it enabled on an RX 7900 XTX (not on this RTX 3060), the title's probes and distant-shadow pass hit geometry in the opening cell. [Ray-tracing test notes](engine/docs/fork-ray-tracing.md)
- **Not verified playable:** nobody has played a session end to end.

</details>

<details>
<summary><b>How it works</b></summary>

ScarlettPort is the **ScarlettPort runtime** (`engine/`, built as `scarlett.exe`) and the **ScarlettPort launcher** (`launcher/`).

```
launcher (ScarlettPort.exe, Rust + egui)
   │  dump folder, presets, SCARLETT_* switches
   ▼
ScarlettPort runtime (scarlett.exe, engine/)
   ├── loader + platform services
   │     the title's x64 code runs natively;
   │     console OS calls answered on Windows
   ├── shader translator
   │     RDNA2 GPU code -> DXIL, cached on disk,
   │     pipelines built ahead of play
   ├── Direct3D 12 back end
   │     command lists, memory, textures, read-backs, presents
   └── DirectStorage + audio
         streaming reads, XMA decode
```

Every runtime switch is a `SCARLETT_*` environment variable, and the runtime keeps its data in `%LOCALAPPDATA%\ScarlettPort`.

Since the runtime's fork point, 97,484 of its 204,657 lines (48%) are new or changed in this project: 145 new files and 94 changed ones, from 411 commits on top of the fork point (295 of them ScarlettPort's own) plus the pull requests merged here. That count also includes community work merged in and credited in [CREDITS.md](CREDITS.md).

### What ScarlettPort has built

- **Runtime work on the frame:** a per-frame timeline of the thread that gates each present, a fair texture upload queue, copies cut from 286 to 84 MB per frame, the yard stutter fixed with lazy remapping of title memory, screen passes with workers of their own, a submission relay and precise read-back settles.
- **Memory work:** sub-page refreshes, a capped card-copy reserve, and opt-in out-of-memory back-off, commit gate and prefetch cap.
- **Picture and shader option sets:** `SCARLETT_LOOK=1` and `SCARLETT_SHADER_FIXES=1`, offered together as the launcher's **Look** preset.
- **Shader pipeline:** a shader prewarm and `scarlett precompile-graphics`.
- **The launcher:** one window to pick a dump, build shaders (resumable, phase by phase), choose a verified preset (Stable, Look) and play; checked updates, controller detection, saves and crash lists.

### What's in here

```
ScarlettPort/
├── engine/     ScarlettPort runtime (builds scarlett.exe)
├── launcher/   native launcher (Rust, egui)
├── runtime/xcl Classic engine: XCL with our changes
├── scripts/    setup/play scripts for both engines
├── patches/    our changes to XCL (xcl-stack.patch)
├── docs/       STATUS, RUNTIMES, BUILDING, ROADMAP,
│               FAQ, TROUBLESHOOTING
├── assets/     screenshots
├── CREDITS.md  credits for the runtime's lineage
└── LICENSE.md  licensing overview
```

</details>

<details>
<summary><b>Screenshots</b></summary>

**Diner cutscene, v0.5.0**

<img src="assets/diner-v050.png" width="100%" alt="Diner cutscene drawn by the ScarlettPort runtime">

<sub>ScarlettPort v0.5.0 on an RTX 3060 (Series S profile, ray tracing off).</sub>

**Gameplay view, lit (earlier build)**

<img src="assets/world-lit.png" width="100%" alt="Lit gameplay view">

**Opening cell on an RTX 3060, default settings**

<img src="assets/cell-rtx3060.png" width="100%" alt="Opening cell on an RTX 3060">

<sub>The v0.4.3 runtime on an RTX 3060 under Windows 10, ray tracing off, while walking and turning: warm rim light through the fence, hair and bunks drawn. Measured at about 5 fps in this scene.</sub>

**Opening cell, ray tracing on (contributor's capture, PR #25)**

Before, then after:

<img src="https://github.com/user-attachments/assets/5e917b82-c0bb-4cbd-bb3e-9dc3b840a42d" width="49%" alt="Cell wall before the lighting fix"> <img src="https://github.com/user-attachments/assets/99a24a6d-f9c3-432a-8d4e-443f88d9568f" width="49%" alt="Cell wall after the lighting fix">
<img src="https://github.com/user-attachments/assets/e1312aa7-9d51-4f8a-9144-28886e0acb9f" width="49%" alt="Bunks before the lighting fix"> <img src="https://github.com/user-attachments/assets/71b78492-79b6-4d29-84f1-b07b8e3ac721" width="49%" alt="Bunks after the lighting fix">
<img src="https://github.com/user-attachments/assets/b13bbf0a-87c7-4928-8f1f-0ce09fda5a47" width="49%" alt="Floor behind the table base with shadows disabled"> <img src="https://github.com/user-attachments/assets/014da3d5-ad7f-4b08-af21-f2dbac7a05de" width="49%" alt="Shadow behind the table base">

<sub>Bounce light on the cell wall and bunk, and the table's shadow, restored by grooving6's PR #25. Captured by the contributor on an RX 7900 XTX with ray tracing switched on (`-RayTracing`, off by default). Not measured on other cards; ray tracing costs frame rate.</sub>

The screenshots below were captured on an RTX 3060 under Windows 10.

**Gameplay view with stand-in lighting (earlier build)**

<img src="assets/world-fallback-light.png" width="100%" alt="World with fallback lighting">

<sub>Before the title's own lighting translated: a stand-in lighting pass (sky, a sun, fog) over the world's own geometry buffers. Fences, a bin and props are drawn; terrain and most surface colours are missing.</sub>

**First gameplay frame (earlier build)**

<img src="assets/tutorial-prompt.png" width="100%" alt="Movement hint prompt">

<sub>The title's own movement hint, drawn over a still-black 3D picture because the lighting shaders did not all translate yet in that build.</sub>

**First world geometry (earlier build)**

<img src="assets/first-world-geometry.png" width="100%" alt="First world geometry">

<sub>One of the title's own render passes drawing the ground in 3D perspective. An internal pass, not a final image.</sub>

**First-boot settings screen**

<img src="assets/menu-subtitles.png" width="100%" alt="Settings screen">

<sub>A first-boot settings screen drawn by the title on the runtime; the window caption shows 30 fps. Other screens (title artwork, loading screens, legal notices) are left out of this repo on purpose.</sub>

</details>

<details>
<summary><b>Switches and presets</b></summary>

0.5.0 turns on by default: `SCARLETT_SAVE_NO_WAIT`, `SCARLETT_CARD_BIG_ROOM` (256 MB), `SCARLETT_GUARD_SINK` and `SCARLETT_LOADS_WIDE`; `=0` turns each off. The **Experimental** preset adds `SCARLETT_LOAD_UNPACED`, `SCARLETT_PREFETCH` and `SCARLETT_PIPELINE_BOOK` (booted cleanly, not measured on their own).

The launcher offers two verified presets: **Stable** (the default) and **Look** (Stable plus `SCARLETT_LOOK=1` and `SCARLETT_SHADER_FIXES=1`; `scripts/play-scarlett.ps1 -Look` does the same). Settings, Features has a performance overlay and a ray tracing switch. All switches: [`docs/RUNTIMES.md`](docs/RUNTIMES.md).

- **`SCARLETT_LOOK=1` (picture):** ambient light so dark rooms are not black, texture and colour fixes, sampler and vertex-format fixes, and ocean textures copied back so boats float instead of sinking.
- **`SCARLETT_SHADER_FIXES=1` (shader translation):** a more accurate translation of many shader instructions and forms. It changes the shader cache key, so the first run with it rebuilds shaders. Each part can be switched on its own (`SCARLETT_SHADER_*`).
- **`SCARLETT_FRIENDLY=1`** (set by Stable): leaves processor time to the desktop while the title runs.
- **`SCARLETT_FAIR_DUE=2`** (default): fair texture upload order. **`SCARLETT_TEX_SPAN_PCT`** (25 by default, `0` turns it off): on a long frame the texture pass may take that share of the frame.
- **`SCARLETT_SCREEN_WORKERS=0`** turns off the screen-pass workers. **`SCARLETT_CARD_ROOM_KEPT_MB`** (512 by default) caps the card-copy reserve.
- **Memory options:** `SCARLETT_OOM_BACKOFF=1`, `SCARLETT_COMMIT_GATE=1`, `SCARLETT_PREFETCH_CAP=1`. Also `SCARLETT_WAVE32_LOWER=1`, `SCARLETT_MOVIE_PLANE=1` and `SCARLETT_THIN=1`.
- **Yard stutter fix:** `SCARLETT_REMAP_LAZY`, `SCARLETT_MEM_SUBPAGE`, `SCARLETT_COPY_NOQUERY`.
- **Testing:** `SCARLETT_AB` (in-session A/B), `SCARLETT_GPU_PROFILE` (GPU frame profiler), `SCARLETT_CAPTURE_LOG` (writes `frames.csv`), `SCARLETT_FAST_BOOT` (optional, off in the presets).

Please send hardware reports if you try the Look preset: what looks better or worse, and your fps.

</details>

<details>
<summary><b>Building from source</b></summary>

Needs [Rust](https://rustup.rs) (MSVC) and the Windows SDK. Full instructions: [`docs/BUILDING.md`](docs/BUILDING.md). Two runtimes are included ([`docs/RUNTIMES.md`](docs/RUNTIMES.md)).

**A. ScarlettPort runtime: furthest today** (lit world)

```powershell
git clone https://github.com/ZigZagBoi1/ScarlettPort.git
cd ScarlettPort
.\scripts\setup-scarlett.ps1
.\scripts\play-scarlett.ps1 -Game "D:\path\to\your\dump"
```

`setup-scarlett.ps1` builds and fetches the Agility SDK; `play-scarlett.ps1` uses the Series S profile by default and the launcher's Stable settings. Its switches include `-AutoInput`, `-FastBoot` (testing only) and `-SaveSlot`.

**B. XCL: menus and loading screens**

```powershell
.\scripts\setup.ps1 -Game "D:\path\to\your\dump"
.\scripts\play.ps1  -Game "D:\path\to\your\dump" -AutoMenu
```

`setup.ps1` builds, fetches the Agility SDK and precompiles shaders (45 min to hours, once; resumes).

- While the game runs, write button names into `press.txt` (e.g. `DOWN DOWN A`) to press them.
- Logs go to `logs\`. Attach one when you open an issue.
- Controls: Xbox controllers and PlayStation pads (DualSense, DualShock 4, over USB or Bluetooth) both work. With no pad connected, the keyboard is a pad: Enter = A, Backspace = B, WASD = left stick, IJKL = right stick. Opt-in `SCARLETT_KBM=1` switches to a PC-style keyboard and mouse layout: `F2` or `` ` `` toggles mouse look (cursor hidden while locked; `Esc`, `P` or losing focus frees it), left/right mouse = RT/LT, middle mouse or `C` = R3, `Shift` = A, `Space` = X, `R` = B, `F` = Y, `Tab` = LB, `Q` = RB, `Ctrl` = L3, `E` = D-pad right, `V` = View; tune with `SCARLETT_MOUSE_SENS` (default `1.0`) and `SCARLETT_MOUSE_INVERT_Y=1`.

**Shader build times** (12-thread CPU)

- **World engine (default):** about 3 min for `scarlett warm` (measured 171 s for 25,889 shaders), then about 8 min for the graphics pipelines (491 s for 25,200).
- **Classic engine (XCL, Settings, Game):** 20-30 min with "skip slow shaders" on; 45 min to several hours with it off.

It resumes: stop it or close the launcher at any time, and building again skips what is done (finished phases are recorded in `<cache>\build-world.json`).

</details>

<details>
<summary><b>Troubleshooting and FAQ</b></summary>

More: [`docs/FAQ.md`](docs/FAQ.md) and [`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md).

**The shader build looks stuck at ~85%.**
That is the Classic (XCL) engine's slow tail: the last ~4,000 of 26,503 programs are huge world shaders that the graphics driver compiles in 6-190 s each, so the count stops moving for minutes at a time while it is still working. Update the launcher: it uses the World engine by default (about 3 minutes), shows the current step and how long it has taken, and with **Skip slow shaders** on (Settings, Options, 60 s) leaves the slowest ones to compile in game. Whatever you already built is kept.

**Do I need to rebuild after updates?**
After a launcher or runtime update the launcher marks the shaders as needing a rebuild. The World engine keys its shader cache on the whole translator source, so an update that changes the shader translator translates everything again (about 3 minutes); one that does not reuses the cache. A per-shader key exists in our fork (`fx-cachekey`) but is not in the shipped runtime yet. The Classic engine's cache is keyed by the generated code, so it recompiles what changed.

**Does the launcher go online?**
Only for what you see: with "Updates" on it asks GitHub Releases at start whether a newer launcher exists. An update downloads only when you click it, into an `update` folder beside the launcher, is checked against the release's SHA-256 list, and is swapped in on the next start; the replaced version is kept in `previous` for a rollback. "Send report" opens a prefilled GitHub hardware-report issue in your browser (your user name and folders are removed from the log). Discord status talks only to the Discord app on your PC.

**Will auto-update trigger antivirus?**
It can. Unsigned apps that download and replace executables can get SmartScreen or antivirus warnings until a release has built up reputation. Our updater only acts when you click it, checks every file against the SHA-256 list, keeps the replaced version in `previous`, and never uses `%TEMP%`. It updates only the launcher and its bundled `runtime` folder; a local build you picked is never touched. If your antivirus flags a release, please report it as a false positive: [Microsoft's submission page](https://www.microsoft.com/en-us/wdsi/filesubmission).

**Can I run my own runtime build?**
Yes: Settings, Runtime, Local build, then browse to its `scarlett.exe` (keep its `D3D12` folder beside it) and pick a cache folder. The Stable preset sets the verified environment, so no play script is needed.

</details>

<details>
<summary><b>Contributing</b></summary>

Read [`CONTRIBUTING.md`](CONTRIBUTING.md), then pick a task from the task board above or open an issue with one of the [issue templates](https://github.com/ZigZagBoi1/ScarlettPort/issues/new/choose) (bug report, hardware report, shader failure).

Testers with different GPUs help most right now: GPU, driver, CPU, RAM, how long the shader build took and how far the game got.

</details>

<details>
<summary><b>Credits and licence</b></summary>

- `engine/` (GPL-3.0-or-later) is a fork, with its lineage and the community work merged in credited in [`CREDITS.md`](CREDITS.md). Fork point: [`engine/UPSTREAM.md`](engine/UPSTREAM.md).
- `runtime/xcl` is XCL by ivall and contributors (GPL-2.0-only).
- Each keeps its own `LICENSE`. See [`LICENSE.md`](LICENSE.md).

</details>

<details>
<summary><b>Disclaimer</b></summary>

- This project is **not affiliated with, endorsed by or sponsored by** the developer or publisher of any title it is used with, or by Microsoft. It is free and is not sold.
- It contains **no game files, code, assets or keys**, and no console SDK code.
- It does **not download game files**. The scripts download only Microsoft's public D3D12 Agility SDK from NuGet.
- You need your **own legally obtained copy** of a title and your own dump of it. This project does not provide, link to or help obtain game files.
- Game and company names are trademarks of their owners.

</details>


---

<div align="center">

**If this is useful, star it ⭐ and share it.** Thank you for more than 500 stars!

<a href="https://star-history.com/#ZigZagBoi1/ScarlettPort&Date"><img src="https://api.star-history.com/svg?repos=ZigZagBoi1/ScarlettPort&type=Date" alt="Star History Chart" width="100%"></a>

</div>
