//! Ready-made HTML animation sources. Claude can also write its own and load them with `add_html`.
//!
//! Contract for an HTML animation clip (so it previews live AND renders frame-exact):
//!  * self-contained (inline CSS/JS, images as data: URIs), transparent background
//!  * animate with CSS animations / Web Animations, or define `window.__seek(t)` (seconds from clip start)
//!  * the page is laid out at the project resolution; use vw/vh/% units

pub fn esc_js(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace("</", "<\\/")
}

pub fn captions(segs: &[(f64, f64, String)], position: &str) -> String {
    let data: Vec<String> = segs
        .iter()
        .map(|(s, e, t)| format!("[{:.3},{:.3},\"{}\"]", s, e, esc_js(t.trim())))
        .collect();
    let pos = match position {
        "top" => "top:12%",
        "middle" | "center" => "top:50%;transform:translateY(-50%)",
        _ => "bottom:14%",
    };
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{{margin:0;width:100%;height:100%;background:transparent;overflow:hidden}}
#c{{position:absolute;left:7%;right:7%;{pos};text-align:center;font:800 5.4vw/1.18 "Segoe UI",system-ui,Arial,sans-serif;color:#fff;
-webkit-text-stroke:0.9vw #000;paint-order:stroke fill;letter-spacing:.01em}}
</style></head><body><div id="c"></div><script>
const S=[{data}];
window.__seek=function(t){{var x=S.find(function(s){{return t>=s[0]&&t<s[1]}});document.getElementById('c').textContent=x?x[2]:''}};
window.__seek(0);
</script></body></html>"#,
        pos = pos,
        data = data.join(",")
    )
}
