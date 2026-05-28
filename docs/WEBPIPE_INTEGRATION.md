# Embedding Algraf In Pipeline Middleware

Algraf can be embedded as a plotting step that receives pipeline state, expands
request-time variables, and returns SVG or PNG bytes. The Algraf source stays
ordinary `.ag` syntax; `Chart(data: input)` marks the host-provided primary data.

```text
GET /svg/weather
|> jq: weatherData
|> jq: `
  .hourly as $h |
  [$h.time, $h.temperature_2m] | transpose | map({time: .[0], temp: .[1]})
`
|> algraf({
  "type": "svg",
  "width": 800,
  "height": 400,
  "dataFormat": "json",
  "variables": {
    "color": "#e74c3c",
    "size": $context.request.query.size // "3"
  }
}): `
  Chart(data: input, width: 800, height: 400) {
    Space(time * temp) {
      Line(stroke: "$color", strokeWidth: $size)
      Point(fill: "$color", size: $size)
    }
  }
`
```

The host is responsible for HTTP routing, request context, and jq evaluation.
Algraf receives only the transformed JSON rows, an inline source string, an
explicit data format, and a variable map.

```rust
use algraf_data::Format;
use algraf_render::{render_embedded, EmbeddedRenderOptions};
use std::collections::HashMap;

fn render_weather_svg(json_rows: &[u8], size: &str) -> Result<String, Box<dyn std::error::Error>> {
    let source = r##"
Chart(data: input, width: 800, height: 400) {
    Space(time * temp) {
        Line(stroke: "$color", strokeWidth: $size)
        Point(fill: "$color", size: $size)
    }
}
"##;

    let result = render_embedded(
        source,
        json_rows,
        EmbeddedRenderOptions {
            data_format: Format::Json,
            variables: HashMap::from([
                ("color".to_string(), "#e74c3c".to_string()),
                ("size".to_string(), size.to_string()),
            ]),
            ..EmbeddedRenderOptions::default()
        },
    )?;

    Ok(result.svg().expect("SVG output").to_string())
}
```

The secure embedded default is input-only: path reads are denied unless the host
provides an explicit `DriverIo` policy. PNG output is returned as `image/png`
bytes; base64 encoding is a host decision.
