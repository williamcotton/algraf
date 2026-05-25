//! Geometry and property analysis (spec §13.6, §13.9–13.13): geometry
//! recognition, per-property type checking, and the dodged-bar hint.

use std::collections::{HashMap, HashSet};

use algraf_core::{codes, Diagnostic, Severity, Span};
use algraf_syntax::ast::{Arg, GeometryCall};
use algraf_syntax::node_span;

use super::context::{ActiveTable, Analyzer, StyleFragmentLookup, ValueForm};
use super::frames::contains_nested;
use crate::ir::*;
use crate::registry::{self, Accept, GeometryDef, PropSpec};
use crate::util::closest;

enum PropOutcome {
    Mapping(ColumnRef),
    Setting(SettingValue),
    Invalid,
}

struct EffectiveArg {
    key: String,
    arg: Arg,
    span: Span,
    from_style: bool,
}

impl Analyzer<'_> {
    // --- Geometry (spec §13.6, §13.9–13.13) ---

    pub(super) fn geometry(
        &mut self,
        call: &GeometryCall,
        frame: &FrameIr,
        table: &ActiveTable,
    ) -> Option<GeometryIr> {
        let span = node_span(call.syntax());
        let name = call.name().unwrap_or_default();

        let def = match registry::geometry(&name) {
            Some(def) => def,
            None => {
                let mut diag =
                    Diagnostic::error(codes::E1201, format!("unknown geometry `{name}`"), span);
                if let Some(suggestion) = closest(&name, registry::geometry_names()) {
                    diag = diag.with_help(format!("did you mean `{suggestion}`?"));
                }
                self.diag(diag);
                return None;
            }
        };

        let args = self.expand_style_args(&call.args());
        let mut seen: HashSet<String> = HashSet::new();
        let mut seen_for_duplicates: HashMap<String, (Span, bool)> = HashMap::new();
        let mut mappings = Vec::new();
        let mut settings = Vec::new();

        for effective in &args {
            let key = &effective.key;
            let key_span = effective.span;

            if let Some((first, first_from_style)) = seen_for_duplicates.get(key).copied() {
                if !first_from_style && !effective.from_style {
                    self.diag(
                        Diagnostic::error(
                            codes::E1203,
                            format!("duplicate property `{key}`"),
                            key_span,
                        )
                        .with_related(first, "first defined here"),
                    );
                    continue;
                }
            }
            seen_for_duplicates.insert(key.clone(), (key_span, effective.from_style));
            seen.insert(key.clone());

            let Some(prop) = def.prop(key) else {
                self.unknown_property(def, key, key_span);
                continue;
            };

            mappings.retain(|mapping: &AestheticMapping| mapping.aesthetic != prop.key);
            settings.retain(|setting: &GeometrySetting| setting.name != prop.key);

            match self.check_property(prop, &effective.arg, table) {
                PropOutcome::Mapping(column) => mappings.push(AestheticMapping {
                    aesthetic: prop.key,
                    column,
                    span: key_span,
                }),
                PropOutcome::Setting(value) => settings.push(GeometrySetting {
                    name: prop.key,
                    value,
                    span: key_span,
                }),
                PropOutcome::Invalid => {}
            }
        }

        for prop in def.props.iter().filter(|p| p.required) {
            if !seen.contains(prop.name) {
                self.diag(Diagnostic::error(
                    codes::E1205,
                    format!("`{}` requires property `{}`", def.name, prop.name),
                    span,
                ));
            }
        }

        self.bar_dodge_hint(def, frame, &mappings, &settings, span);

        Some(GeometryIr {
            kind: def.kind,
            mappings,
            settings,
            span,
        })
    }

    fn expand_style_args(&mut self, args: &[Arg]) -> Vec<EffectiveArg> {
        let mut out = Vec::new();
        for arg in args {
            let Some(key) = arg.key() else { continue };
            let span = node_span(arg.syntax());
            if key != "style" {
                out.push(EffectiveArg {
                    key,
                    arg: arg.clone(),
                    span,
                    from_style: false,
                });
                continue;
            }
            let Some(value) = arg.value() else {
                self.diag(Diagnostic::error(
                    codes::E1706,
                    "`style` expects a `Style(...)` fragment",
                    span,
                ));
                continue;
            };
            match self.style_fragment_for_value(&value) {
                StyleFragmentLookup::Found(entries) => {
                    for entry in entries {
                        out.push(EffectiveArg {
                            key: entry.key,
                            arg: entry.arg,
                            span: entry.span,
                            from_style: true,
                        });
                    }
                }
                StyleFragmentLookup::NotStyle => {
                    self.diag(Diagnostic::error(
                        codes::E1706,
                        "`style` expects a `Style(...)` fragment",
                        node_span(value.syntax()),
                    ));
                }
                StyleFragmentLookup::Invalid => {}
            }
        }
        out
    }

    fn unknown_property(&mut self, def: &GeometryDef, key: &str, span: Span) {
        let mut diag = Diagnostic::error(
            codes::E1202,
            format!("unknown property `{key}` on `{}`", def.name),
            span,
        );
        if key.eq_ignore_ascii_case("colour") || key.eq_ignore_ascii_case("color") {
            diag = diag.with_help(
                "choose `fill` or `stroke`; `colour` is not an alias because they differ",
            );
        } else if let Some(suggestion) = closest(key, def.prop_names()) {
            diag = diag.with_help(format!("did you mean `{suggestion}`?"));
        }
        self.diag(diag);
    }

    fn check_property(&mut self, prop: &PropSpec, arg: &Arg, table: &ActiveTable) -> PropOutcome {
        let Some(value) = arg.value() else {
            return PropOutcome::Invalid;
        };
        // Resolve `let` variables in property value positions before type
        // checking, so a bound constant is checked as its value (spec §9.6).
        let form = self.substitute_var(ValueForm::of(&value));

        // Color literals written as bare identifiers (e.g. `fill: red`) are a
        // common mistake. If this property accepts a color and the value is a
        // bare identifier that names a known CSS color but no such column
        // exists, emit a hint to quote it (H3002).
        if prop.accepts.contains(&Accept::Color) {
            if let ValueForm::Column(name) = &form {
                let raw = name.name().unwrap_or_default();
                if !name.is_quoted() && table.get(&raw).is_none() && is_css_color_name(&raw) {
                    self.diag(
                        Diagnostic::new(
                            Severity::Hint,
                            codes::H3002,
                            format!("quote literal color name `{raw}` for clarity"),
                            node_span(name.syntax()),
                        )
                        .with_help(format!("write it as a string, e.g. {raw:?}")),
                    );
                }
            }
        }

        for accept in prop.accepts {
            match (accept, &form) {
                (Accept::Column, ValueForm::Column(name)) => {
                    return PropOutcome::Mapping(self.resolve_column(name, table));
                }
                (Accept::Number, ValueForm::Number(n)) => {
                    return PropOutcome::Setting(SettingValue::Number(*n));
                }
                (Accept::Color | Accept::Str, ValueForm::Str(s)) => {
                    return PropOutcome::Setting(SettingValue::String(s.clone()));
                }
                (Accept::Bool, ValueForm::Bool(b)) => {
                    return PropOutcome::Setting(SettingValue::Bool(*b));
                }
                (Accept::Enum(opts), ValueForm::Str(s)) if opts.contains(&s.as_str()) => {
                    return PropOutcome::Setting(SettingValue::String(s.clone()));
                }
                (Accept::NumberArray, ValueForm::Array(Some(nums))) => {
                    return PropOutcome::Setting(SettingValue::NumberArray(nums.clone()));
                }
                _ => {}
            }
        }

        // No accepted form matched: produce a precise type diagnostic.
        let span = node_span(value.syntax());
        let enum_opts = prop.accepts.iter().find_map(|a| match a {
            Accept::Enum(opts) => Some(*opts),
            _ => None,
        });
        if let (Some(opts), ValueForm::Column(name)) = (enum_opts, &form) {
            let written = name.name().unwrap_or_else(|| opts[0].to_string());
            self.diag(
                Diagnostic::error(
                    codes::E1204,
                    format!("`{}` expects a quoted string value", prop.name),
                    span,
                )
                .with_help(format!("write it as a string, e.g. {written:?}")),
            );
        } else {
            self.diag(Diagnostic::error(
                codes::E1204,
                format!(
                    "`{}` expects {}, found {}",
                    prop.name,
                    describe_accepts(prop.accepts),
                    form.describe()
                ),
                span,
            ));
        }
        PropOutcome::Invalid
    }

    /// Suggest nested algebra for dodged bars (hint H3001).
    fn bar_dodge_hint(
        &mut self,
        def: &GeometryDef,
        frame: &FrameIr,
        mappings: &[AestheticMapping],
        settings: &[GeometrySetting],
        span: Span,
    ) {
        if def.kind != GeometryKind::Bar {
            return;
        }
        let has_fill = mappings.iter().any(|m| m.aesthetic == PropertyKey::Fill);
        let stacked = settings.iter().any(|s| {
            s.name == PropertyKey::Layout
                && matches!(&s.value, SettingValue::String(v) if v != "identity")
        });
        // Only hint when the space is a flat Cartesian with no nesting; a
        // frame that already nests is the dodge form the hint would suggest.
        let plain_cartesian = matches!(frame, FrameIr::Cartesian(_)) && !contains_nested(frame);
        if has_fill && plain_cartesian && !stacked {
            self.diag(
                Diagnostic::new(
                    Severity::Hint,
                    codes::H3001,
                    "use nested algebra for dodged bars",
                    span,
                )
                .with_help("e.g. `Space((x / fill) * y)`, or set `layout: \"stack\"`"),
            );
        }
    }
}

fn describe_accepts(accepts: &[Accept]) -> String {
    let parts: Vec<String> = accepts
        .iter()
        .map(|a| match a {
            Accept::Column => "a column mapping".to_string(),
            Accept::Number => "a number".to_string(),
            Accept::Color => "a color string".to_string(),
            Accept::Str => "a string".to_string(),
            Accept::Bool => "a boolean".to_string(),
            Accept::Enum(opts) => format!("one of {opts:?}"),
            Accept::NumberArray => "an array of numbers".to_string(),
        })
        .collect();
    parts.join(" or ")
}

/// Whether `name` is a commonly used CSS color keyword (for H3002 hints).
fn is_css_color_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "red"
            | "green"
            | "blue"
            | "yellow"
            | "black"
            | "white"
            | "gray"
            | "grey"
            | "orange"
            | "purple"
            | "pink"
            | "brown"
            | "cyan"
            | "magenta"
            | "lime"
            | "navy"
            | "teal"
            | "maroon"
            | "olive"
            | "silver"
            | "gold"
            | "steelblue"
            | "tomato"
            | "salmon"
            | "indigo"
            | "violet"
            | "turquoise"
            | "coral"
            | "crimson"
            | "khaki"
            | "plum"
    )
}

pub(super) fn is_color_literal(value: &str) -> bool {
    is_hex_color(value) || is_css_color_name(value)
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6) && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}
