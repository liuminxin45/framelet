// Development-only interoperability test using the official MCP TypeScript SDK.
import { pathToFileURL } from 'node:url';
import path from 'node:path';
import fs from 'node:fs/promises';
import assert from 'node:assert/strict';

const sdk = path.resolve('work/mcp-client/node_modules/@modelcontextprotocol/sdk/dist/esm');
const { Client } = await import(pathToFileURL(path.join(sdk, 'client/index.js')));
const { StdioClientTransport } = await import(pathToFileURL(path.join(sdk, 'client/stdio.js')));
const executable = path.resolve(process.argv[2]);
const input = path.resolve(process.argv[3] && !process.argv[3].startsWith('--') ? process.argv[3] : 'docs/images/framelet-demo.gif');
const desktop = process.argv.includes('--desktop');
await fs.mkdir('work', { recursive: true });
const output = await fs.mkdtemp(path.resolve('work/mcp-check-'));
const client = new Client({ name: 'framelet-interoperability-check', version: '1.0.0' }, { capabilities: {} });
const transport = new StdioClientTransport({ command: executable, args: ['mcp'], stderr: 'pipe', env: { ...process.env, FRAMELET_MCP_ROOT: output, FRAMELET_MCP_READ_ROOTS: JSON.stringify([path.dirname(input)]) } });
const checks = [];
async function call(name, args) {
  const r = await client.callTool({ name, arguments: args });
  assert.notEqual(r.isError, true, JSON.stringify(r.content));
  assert.equal(r.structuredContent.ok, true);
  return r;
}
function image(r) {
  const block = r.content.find(c => c.type === 'image');
  assert(block, 'Expected an MCP image content block');
  assert.equal(block.mimeType, 'image/png');
  const bytes = Buffer.from(block.data, 'base64');
  assert.equal(bytes.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  return bytes;
}
try {
  await client.connect(transport);
  const tools = (await client.listTools()).tools;
  assert.equal(tools.length, 20);
  assert(!tools.some(t => t.name === 'framelet_record'));
  checks.push('SDK initialization and tool discovery');
  {
    const context = (await call('framelet_context', {})).structuredContent.result;
    const guidance = context.capture_guidance;
    assert(context.documentation.timeline && context.timeline.default_enabled);
    assert.deepEqual(guidance.fps_range, { min: 1, max: 60 });
    assert.deepEqual(guidance.presets.map(p => [p.intent, p.fps]), [
      ['static_state_check', 1], ['development_workflow', 5],
      ['compact_documentation', 12], ['presentation', 30], ['fast_motion_check', 50]
    ]);
    for (const preset of guidance.presets) {
      assert(preset.description_en && /[\u4e00-\u9fff]/.test(preset.description_zh));
    }
    assert(guidance.selection_rule && guidance.performance_rule && guidance.timing_note);
    checks.push('machine-readable bilingual FPS presets and capture caveats');
  }

  const info = (await call('framelet_inspect', { input })).structuredContent.result;
  assert(info.frames > 0 && info.duration_ms >= 300);
  const edited = await call('framelet_edit', { input, output: path.join(output, 'trim.gif'), start: 0.1, end: 0.3 });
  assert.equal(edited.structuredContent.result.duration_ms, 200);
  checks.push('SDK inspect and trim');
  const invalid = await client.callTool({ name: 'framelet_inspect', arguments: { input, arbitrary: true } });
  assert.equal(invalid.isError, true);
  await client.ping();
  checks.push('tool errors preserve the MCP session');
  {
    const point = await call('framelet_frame_at', { input, at: 0.12, output: path.join(output, 'point.png') });
    image(point);
    assert.equal(point.structuredContent.result.preview.source_width,info.width);
    assert(point.structuredContent.result.timestamp_ms <= 120);
    assert(point.structuredContent.result.timestamp_ms + point.structuredContent.result.duration_ms > 120);
    checks.push('single timestamp probe returns an MCP image and its exact playback interval');
    const filesBefore=await fs.readdir(output);
    const numeric=await call('framelet_frame_at',{input,at:0.12,reference:path.join(output,'point.png'),compare_region:{x:0,y:0,width:16,height:16},include_image:false});
    assert.equal(numeric.content.filter(c=>c.type==='image').length,0);
    assert.equal(numeric.structuredContent.result.comparison.changed_fraction,0);
    assert.equal(numeric.structuredContent.result.comparison.mean_absolute_error,0);
    assert.deepEqual(await fs.readdir(output),filesBefore);
    const badFlag=await client.callTool({name:'framelet_frame_at',arguments:{input,at:0.12,include_image:'false'}});
    assert.equal(badFlag.isError,true);
    const badThreshold=await client.callTool({name:'framelet_frame_at',arguments:{input,at:0.12,reference:path.join(output,'point.png'),include_image:false,difference_threshold:256}});
    assert.equal(badThreshold.isError,true);
    checks.push('numeric region probe sends zero images, writes zero PNGs and validates flags');
    const extraction = await call('framelet_extract_frames', { input, output: path.join(output, 'frames'), count: 8, mode: 'uniform' });
    await fs.writeFile(path.join(output, 'mcp-contact-sheet.png'), image(extraction));
    const report = extraction.structuredContent.result;
    assert(report.frames.length > 0 && report.semantic_analysis === false);
    const selected = await call('framelet_read_image', { input: report.frames[Math.floor(report.frames.length / 2)].image });
    await fs.writeFile(path.join(output, 'mcp-selected-frame.png'), image(selected));
    const badPng = await client.callTool({ name: 'framelet_read_image', arguments: { input } });
    assert.equal(badPng.isError, true);
    await client.ping();
    checks.push('extraction contact sheet, selected image and non-PNG error via SDK');
  }
  if (desktop) {
    assert.equal((await call('framelet_doctor', {})).structuredContent.result.capture_probe, true);
    const monitors = (await call('framelet_monitors', {})).structuredContent.result;
    const monitor = monitors.find(m => m.primary) ?? monitors[0];
    const region = { x: monitor.region.x, y: monitor.region.y, width: 128, height: 96 };
    await call('framelet_windows', {});
    const shot = await call('framelet_shot', { region, output: path.join(output, 'shot.png') });
    image(shot);
    console.log(JSON.stringify({ shot_image_blocks: shot.content.filter(c => c.type === 'image').length }));
    const started = (await call('framelet_start_recording', { region, duration: 0.6, fps: 10, output: path.join(output, 'capture.gif') })).structuredContent.result;
    let status;
    const deadline = Date.now() + 20000;
    do {
      assert(Date.now() < deadline, 'recording terminal timeout');
      await new Promise(resolve => setTimeout(resolve, 50));
      status = (await call('framelet_recording_status', {session_id:started.session_id})).structuredContent.result;
    } while (!status.terminal);
    assert.equal(status.result.state, 'completed');
    const recording = {structuredContent:{result:status.result.recording}};
    assert(Math.abs(recording.structuredContent.result.duration_ms - 600) <= 30);
    const timeline=await call('framelet_timeline',{input:recording.structuredContent.result.output,at:0.1});
    assert(timeline.structuredContent.result.sample_count>0 && timeline.structuredContent.result.sample);
    const wrongTime=await client.callTool({name:'framelet_timeline',arguments:{input:recording.structuredContent.result.output,at:'wrong'}});
    assert.equal(wrongTime.isError,true);
    checks.push('default live timeline, MCP time query and invalid query validation');
    checks.push('SDK doctor, monitors, windows, live screenshot and recording');
  }
  console.log(JSON.stringify({ ok: true, server: client.getServerVersion(), tools: tools.map(t => t.name), checks, artifacts: output }));
} finally {
  await client.close();
}
