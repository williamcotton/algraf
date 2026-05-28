use crate::error::WebPipeError;
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;

use algraf_data::Format;
use algraf_render::{
    render_embedded, EmbeddedOutputFormat, EmbeddedRenderError, EmbeddedRenderOptions,
};

#[derive(Debug)]
pub struct AlgrafMiddleware;

impl AlgrafMiddleware {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlgrafConfig {
    #[serde(rename = "type", default)]
    output_type: AlgrafOutputType,

    #[serde(default = "default_data_format")]
    data_format: FormatOption,

    #[serde(default)]
    width: Option<u32>,

    #[serde(default)]
    height: Option<u32>,

    #[serde(default)]
    theme: Option<String>,

    #[serde(default)]
    strict: bool,

    #[serde(default)]
    png_scale: Option<f32>,

    #[serde(default)]
    png_dpi: Option<u32>,

    #[serde(default)]
    variables: HashMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum AlgrafOutputType {
    Svg,
    Png,
}

impl Default for AlgrafOutputType {
    fn default() -> Self {
        AlgrafOutputType::Svg
    }
}

impl From<AlgrafOutputType> for EmbeddedOutputFormat {
    fn from(value: AlgrafOutputType) -> Self {
        match value {
            AlgrafOutputType::Svg => EmbeddedOutputFormat::Svg,
            AlgrafOutputType::Png => EmbeddedOutputFormat::Png,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct FormatOption(String);

impl FormatOption {
    fn parse(&self) -> Result<Format, WebPipeError> {
        Format::from_str(&self.0).map_err(|message| {
            WebPipeError::MiddlewareExecutionError(format!("Invalid Algraf dataFormat: {message}"))
        })
    }
}

fn default_data_format() -> FormatOption {
    FormatOption("json".to_string())
}

#[async_trait]
impl super::Middleware for AlgrafMiddleware {
    async fn execute(
        &self,
        args: &[String],
        config: &str,
        pipeline_ctx: &mut crate::runtime::PipelineContext,
        env: &crate::executor::ExecutionEnv,
        ctx: &mut crate::executor::RequestContext,
        _target_name: Option<&str>,
    ) -> Result<super::MiddlewareOutput, WebPipeError> {
        let (render_options, variables) = if let Some(arg_expr) = args.first() {
            let combined_input = serde_json::json!({
                "state": pipeline_ctx.state,
                "context": ctx.to_value(env),
            });
            let wrapped_expr = format!(".context as $context | .state | ({})", arg_expr);
            let cfg_value = crate::runtime::jq::evaluate(&wrapped_expr, &combined_input)?;
            let cfg: AlgrafConfig = serde_json::from_value(cfg_value).map_err(|e| {
                WebPipeError::MiddlewareExecutionError(format!("Invalid Algraf config: {e}"))
            })?;
            let variables = variables_to_strings(cfg.variables);
            (cfg.into_render_options()?, variables)
        } else {
            let variables = HashMap::new();
            (
                AlgrafConfig::default_for_inline_json().into_render_options()?,
                variables,
            )
        };

        let mut options = render_options;
        options.variables = variables;

        let input_bytes = input_bytes_for_format(&pipeline_ctx.state, options.data_format)?;
        let result = render_embedded(config, input_bytes, options).map_err(algraf_error)?;

        pipeline_ctx.state = match result.content_type {
            "image/svg+xml" => {
                let svg = String::from_utf8(result.bytes).map_err(|_| {
                    WebPipeError::MiddlewareExecutionError(
                        "Generated Algraf SVG was not valid UTF-8".to_string(),
                    )
                })?;
                Value::String(svg)
            }
            "image/png" => Value::String(general_purpose::STANDARD.encode(result.bytes)),
            other => {
                return Err(WebPipeError::MiddlewareExecutionError(format!(
                    "Unsupported Algraf content type: {other}"
                )));
            }
        };

        Ok(super::MiddlewareOutput {
            content_type: Some(result.content_type.to_string()),
        })
    }

    fn behavior(&self) -> super::StateBehavior {
        super::StateBehavior::Transform
    }
}

impl AlgrafConfig {
    fn default_for_inline_json() -> Self {
        AlgrafConfig {
            output_type: AlgrafOutputType::Svg,
            data_format: default_data_format(),
            width: None,
            height: None,
            theme: None,
            strict: false,
            png_scale: None,
            png_dpi: None,
            variables: HashMap::new(),
        }
    }

    fn into_render_options(self) -> Result<EmbeddedRenderOptions, WebPipeError> {
        let mut options = EmbeddedRenderOptions {
            data_format: self.data_format.parse()?,
            width: self.width,
            height: self.height,
            theme: self.theme,
            output_format: self.output_type.into(),
            strict: self.strict,
            png_dpi: self.png_dpi,
            ..EmbeddedRenderOptions::default()
        };

        if let Some(scale) = self.png_scale {
            options.png_scale = scale;
        }

        Ok(options)
    }
}

fn variables_to_strings(variables: HashMap<String, Value>) -> HashMap<String, String> {
    variables
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                Value::String(value) => value,
                Value::Number(value) => value.to_string(),
                Value::Bool(value) => value.to_string(),
                Value::Null => String::new(),
                other => other.to_string(),
            };
            (key, value)
        })
        .collect()
}

fn input_bytes_for_format(value: &Value, format: Format) -> Result<Vec<u8>, WebPipeError> {
    match (format, value) {
        (Format::Csv | Format::Tsv | Format::NdJson, Value::String(text)) => {
            Ok(text.as_bytes().to_vec())
        }
        _ => serde_json::to_vec(value).map_err(|e| {
            WebPipeError::MiddlewareExecutionError(format!(
                "Invalid Algraf pipeline state input: {e}"
            ))
        }),
    }
}

fn algraf_error(error: EmbeddedRenderError) -> WebPipeError {
    match error {
        EmbeddedRenderError::Usage(message)
        | EmbeddedRenderError::Driver(message)
        | EmbeddedRenderError::Render(message) => {
            WebPipeError::MiddlewareExecutionError(format!("Algraf Render Error: {message}"))
        }
        EmbeddedRenderError::Diagnostics {
            diagnostics,
            data_warnings,
        } => {
            let mut parts = diagnostics
                .into_iter()
                .map(|diagnostic| {
                    format!(
                        "{} {:?}: {}",
                        diagnostic.code, diagnostic.severity, diagnostic.message
                    )
                })
                .collect::<Vec<_>>();
            parts.extend(
                data_warnings
                    .into_iter()
                    .map(|warning| format!("warning: {}", warning.message())),
            );
            WebPipeError::MiddlewareExecutionError(format!(
                "Algraf diagnostics blocked rendering: {}",
                parts.join("; ")
            ))
        }
    }
}
