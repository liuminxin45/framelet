# GifCam investigation and Framelet reconstruction

Date: 2026-10-30. This is an evidence-based implementation note, not a claim of complete source recovery or feature equivalence.

## Target provenance

- Official download: https://www.bahraniapps.com/apps/gifcam/gifcam.php
- The supplied URL returned a ZIP directly, rather than an HTML page.
- Downloaded ZIP: 759,165 bytes. Extracted executable: 1,771,520 bytes.
- Windows version resource: GifCam 7.7.0.0.
- Executable SHA-256: `5009c67b397f6857c82d228cddb75c12b6a186cde099e5557ed2a46acb94c438`.
- PE32 x86, image base `0x400000`, PE entry `0x567580`.

The reference executable, private analysis snapshots and raw decompiler output are kept outside this source repository. They are not redistributed in Framelet's source or release package.

## Tools deployed

Official [morluto/rea](https://github.com/morluto/rea), npm `rea-agents` 6.8.0; isolated Node 24.22.2; [Ghidra 12.1.4](https://github.com/NationalSecurityAgency/ghidra/releases/tag/Ghidra_12.1.4_build); Temurin JDK 21.0.13+9. REA is registered for Codex, and a `rea.cmd` launcher selects the isolated runtime and provider paths.

Repeated interrupted downloads were replaced by HTTP range extraction from the official archives. The selected runtime contains Windows/x86 Ghidra components and the JDK runtime/compiler. Other Ghidra processor packages, optional extensions, source bundles and FunctionID databases were omitted. ZIP entry CRC32 was checked during extraction; no claim is made that an entire downloaded archive passed its published SHA-256. The deployment inventory records SHA-256 of the installed selected files. REA's scoped doctor passed Node, JDK, headless launcher, version, Windows native decompiler, Job Object/DACL/path admission and Codex registration checks. This deployment is verified for this Windows/x86 target.

Ghidra import used `x86:LE:32:default` and `borlanddelphi`. The installed language definitions explicitly include the Delphi compiler specification and its EAX/EDX/ECX register convention. These target-specific settings are applied only to the analysis command, rather than forcing all future REA targets to use Delphi.

## Observations

The running application exposes a transparent capture aperture and native Rec, Frame, Edit and Save controls. The Rec menu exposes New, Open, 10/16/33 FPS, Draw Selection, Full Screen, Capture Cursor and Customization. This guided the compact frame interaction and CLI region model.

REA resource inspection found VCL markers `DVCLAL`, `PACKAGEINFO` and form resources including `TGIFCAMFORM`, `TEDITFORM`, `TPREVIEWFORM`, `TCROPFORM`, `TADDTEXTFORM`, `TCOLORMAPSFORM` and `TRESIZEFORM`. Together with the x86 layout and compiler convention, these support a Delphi/VCL interpretation. Resource names identify feature boundaries; they do not prove an algorithm's behavior.

Ghidra completed default auto-analysis and reported **5,214 functions** and **4,110 defined strings**, across 11 loaded segments. These are Ghidra's discovered inventories; they do not establish complete recovery of all functions or undefined strings.

| API | IAT VA | Import thunk VA | Example callsite VA |
| --- | --- | --- | --- |
| BitBlt | 0x7b7078 | 0x40d618 | 0x5476f7 |
| CreateDIBSection | 0x7b7058 | 0x40d658 | 0x547670 |
| GetDIBits | 0x7b6fec | 0x40d730 | 0x548501 |

REA directly observed import entries. Supplementary pefile/Capstone analysis located the import thunks and direct-call candidates above by linear x86 disassembly. These references support GDI bitmap processing and are not a runtime trace, nor proof that every such call belongs to recording. REA native analysis confirms the file and compiler profile separately.

Canonical evidence IDs:

- Desktop inspection: `ev_7fb856e6faefe95dae3c1e16d215df2bdb8ebc85db373f0976c40034fc3388cc`.
- PE imports: `ev_08be854ec028ee92334cd1677ef7dd6e442c8519ef52d17559df6868c0f49871`.
- PE resources: `ev_98d0cbc6e312d067622b3ca69ca0e8789952b566b950d29bacaecc394d5cf716`.
- Native overview: `ev_cdba03d01406a8c2c6992978eb677e96dbe612db5c9d6ae8b554539e701156b0`.
- Ghidra decompilation at `0x5474a4`: `ev_f67160378fb5dc84df3b8c691cbf9e9c15c55d8432582899275da5f0f1da139e`.

The `0x5474a4` function was successfully decompiled with the Delphi profile. It derives width/height from a supplied rectangle, creates a compatible DC and a DIB section, selects that bitmap, calls `BitBlt` with the supplied source DC and `SRCCOPY`, and processes pixel formats/resampling/compositing before cleanup. This is concrete evidence for a bitmap-processing boundary. The decoded function includes unresolved symbols and an overlapping-global warning; it is not established as the main recording loop. Its raw pseudocode remains in the separate private investigation bundle and was not copied into the implementation.

## Limits and implementation decisions

REA's separate PE load-configuration parser rejected this target with `Virtual address or string range is not file mapped`. That operation is recorded as unsuccessful. It does not invalidate the successful import/resource inspections or the separately completed Ghidra import.

Framelet is independently written Rust/Win32 code. The native-frame pattern, monitor/region semantics and bitmap capture boundary informed the implementation. Streaming GIF output, local color quantization, static-frame merging and bounding-box delta crops are implementation choices; the investigation does not establish that they reproduce GifCam's internal encoder exactly. No reference icons, form assets or decompiled functions are included in Framelet.

Current parity: capture frame, region recording, pause/single-frame workflow, mouse cursor, GIF output and save dialog. CLI adds PNG, JSON results, progress events, named stop events, trim/speed and MCP. GifCam's visual timeline, drawing/text overlays, per-frame deletion, richer color controls and AVI are not implemented in v0.1. Audio is not supported.

## Verification

Rust format check, Clippy with warnings denied and unit tests pass. Independent Pillow checks cover rendered GIF pixels and timing, cropping/speed changes, no-overwrite behavior, invalid input and cleanup. Real desktop tests cover PNG/GIF recording and cross-process named stop; MCP checks cover initialization, discovery, successful invocation and invalid arguments. Native UI checks exercise Record, Pause, F8 stop, single-frame capture and Save As; the exported single-frame GIF was independently decoded.

An actual 620×300, 3-second, 16 FPS capture produced 48 frames and a 15,250-byte GIF. This is a measurement of a mostly static UI with a moving marker, not a general compression benchmark or comparison against GifCam. Windows CI is configured; no remote CI execution is claimed by these local results.
