# 60 FPS capture / 60 FPS 采集

Available in v0.3.0-rc.1. CLI/MCP accept 1–60 FPS; the native capture frame also offers 30/50/60 FPS. Existing defaults stay unchanged.

```powershell
framelet record --region 0,0,960,540 --fps 60 --duration 5 --output demo.gif
framelet record-series --output-dir D:\Captures --name demo --duration 20 --fps 60 --region 0,0,960,540
```

MCP uses fps:60 on framelet_record, framelet_start_recording or framelet_start_series. Call framelet_context first. This version still has one FPS parameter; separate capture_fps/gif_fps and four visual presets are a [design proposal](fidelity-strategy-design.md).

## What 60 means / 60 的含义

- FPS is a sampling target, not a guarantee for every resolution or display. Check actual sample count and capture intervals. Repeated screen pixels may merge into fewer GIF frames.
- GIF delay units are 10ms. 60 FPS uses mixed 10/20ms delays with cumulative timing; it does not provide uniform 16.67ms intervals.
- Chromium currently extends GIF frame delays of <=10ms to 100ms. A valid 60-FPS GIF can therefore play slower in browsers. Use native timestamp probes/frame extraction for high-rate inspection; use 30 FPS for browser/README sharing.
- Recording results include playback.requested_capture_fps, gif_delay_unit_ms, short_delay_frames and browser_timing_warning. This is compatibility information, not a failed or incomplete input trace; it does not turn a successful rotating recording into partial.
- The 80 MP/s transformed/encoded-pixel guard remains. 1280×720 at 60 FPS fits that numeric budget; 1920×1080 at 60 FPS does not. Actual performance still depends on capture area, source updates, CPU and color complexity.
- Increasing capture rate cannot recover changes that were never displayed. Display refresh/source update rate and requested sampling are separate quantities.

## Verification / 验证

```powershell
python scripts/verify_60fps.py target/release/framelet.exe --output work/fps60-check --require-rate
```

This checks real masked GDI capture, CLI/MCP range validation, native timeline integrity and 60-FPS rotating output. The optional rate gate requires at least 54 samples/second on this workload; it is not enabled for generic hosted CI performance comparisons. Independent synthetic-frame tests verify 10/20ms encoding and exact total duration; dynamic browser-scene acceptance additionally checks distinct encoded images.

References: [GIF89a timing](https://www.w3.org/Graphics/GIF/spec-gif89a.txt), [Chromium delay policy](https://raw.githubusercontent.com/chromium/chromium/main/third_party/blink/renderer/platform/graphics/deferred_image_decoder.cc).
