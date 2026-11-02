use crate::{
    capture::{self, Grabber, Region},
    encode,
};
use anyhow::Result;
use serde_json::{Value, json};
pub fn doctor() -> Value {
    let d = capture::desktop();
    let probe = Grabber::new(
        Region {
            x: d.x,
            y: d.y,
            width: 1,
            height: 1,
        },
        false,
    )
    .and_then(|g| g.rgb());
    json!({"ok":probe.is_ok(),"version":env!("CARGO_PKG_VERSION"),"platform":"windows-x64","backend":"GDI BitBlt + DIB","desktop":d,"capture_probe":probe.is_ok(),"error":probe.err().map(|e|e.to_string()),"formats":["gif","png"],"network_required":false,"audio":false})
}
pub fn shot(region: Option<Region>, path: &std::path::Path, cursor: bool) -> Result<Value> {
    let r = region.unwrap_or_else(capture::desktop);
    let g = Grabber::new(r, cursor)?;
    encode::screenshot(path, r.width, r.height, &g.rgb()?)
}
