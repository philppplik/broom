//! Interpreter for winutil's tweak catalog (MIT, (c) Chris Titus Tech / CT Tech Group LLC).
//! The JSON files in third_party/winutil are embedded unmodified.

use super::{Op, Tweak};
use crate::util::reg::Val;
use crate::util::Risk;
use serde::Deserialize;
use std::collections::BTreeMap;

const TWEAKS: &str = include_str!("../../third_party/winutil/tweaks.json");
const FEATURES: &str = include_str!("../../third_party/winutil/feature.json");
const PRESETS: &str = include_str!("../../third_party/winutil/preset.json");
pub const APPX: &str = include_str!("../../third_party/winutil/appx.json");
pub const DNS: &str = include_str!("../../third_party/winutil/dns.json");

/// winutil's files contain raw newlines/tabs inside strings; escape them so serde accepts the JSON.
pub fn lenient(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let (mut in_str, mut esc) = (false, false);
    for c in json.trim_start_matches('\u{feff}').chars() {
        if in_str {
            if esc {
                esc = false;
                out.push(c);
                continue;
            }
            match c {
                '\\' => {
                    esc = true;
                    out.push(c);
                }
                '"' => {
                    in_str = false;
                    out.push(c);
                }
                '\n' => out.push_str("\\n"),
                '\r' => {}
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => {}
                c => out.push(c),
            }
        } else {
            if c == '"' {
                in_str = true;
            }
            out.push(c);
        }
    }
    out
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RegEntry {
    #[serde(rename = "Path")]
    path: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Value")]
    value: serde_json::Value,
    #[serde(rename = "Type")]
    ty: String,
    #[serde(rename = "OriginalValue")]
    original: serde_json::Value,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SvcEntry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "StartupType")]
    startup: String,
    #[serde(rename = "OriginalType")]
    original: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TaskEntry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "OriginalState")]
    original: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Entry {
    #[serde(rename = "Content")]
    content: String,
    #[serde(rename = "Description")]
    description: String,
    category: String,
    link: String,
    #[serde(rename = "Type")]
    ty: String,
    registry: Vec<RegEntry>,
    service: Vec<SvcEntry>,
    #[serde(rename = "ScheduledTask")]
    tasks: Vec<TaskEntry>,
    #[serde(rename = "InvokeScript")]
    invoke: Vec<String>,
    #[serde(rename = "UndoScript")]
    undo: Vec<String>,
    feature: Vec<String>,
}

fn json_str(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some((*b as u8).to_string()),
        _ => None,
    }
}

fn reg_op(r: &RegEntry, value: &serde_json::Value) -> Option<Op> {
    let v = json_str(value)?;
    if v == "<RemoveEntry>" {
        return Some(Op::RegDel { path: r.path.clone(), name: r.name.clone() });
    }
    Some(Op::RegSet { path: r.path.clone(), name: r.name.clone(), val: Val::from_typed(&r.ty, &v)? })
}

fn category(c: &str) -> (&'static str, Risk) {
    let l = c.to_lowercase();
    if l.contains("essential") {
        ("Essential (winutil)", Risk::Safe)
    } else if l.contains("customize") || l.contains("preferences") {
        ("Preferences (winutil)", Risk::Safe)
    } else if l.contains("performance plans") {
        ("Power", Risk::Moderate)
    } else {
        ("Advanced (winutil)", Risk::Aggressive)
    }
}

pub fn tweaks() -> Vec<Tweak> {
    let map: BTreeMap<String, Entry> = match serde_json::from_str(&lenient(TWEAKS)) {
        Ok(m) => m,
        Err(e) => {
            crate::util::log(format!("winutil tweaks.json parse error: {e}"));
            return vec![];
        }
    };
    let mut v = Vec::new();
    for (id, e) in map {
        if e.content.is_empty() || e.ty == "Button" || e.ty == "Combobox" {
            continue;
        }
        let (cat, risk) = category(&e.category);
        let mut apply: Vec<Op> = e.registry.iter().filter_map(|r| reg_op(r, &r.value)).collect();
        let mut undo: Vec<Op> = e.registry.iter().filter_map(|r| reg_op(r, &r.original)).collect();
        for s in &e.service {
            apply.push(Op::Service { name: s.name.clone(), start: s.startup.clone() });
            if !s.original.is_empty() {
                undo.push(Op::Service { name: s.name.clone(), start: s.original.clone() });
            }
        }
        for t in &e.tasks {
            apply.push(Op::Task { name: t.name.clone(), enable: t.state.eq_ignore_ascii_case("Enabled") });
            undo.push(Op::Task { name: t.name.clone(), enable: t.original.eq_ignore_ascii_case("Enabled") });
        }
        if !e.invoke.is_empty() {
            apply.push(Op::Ps(e.invoke.join("\n")));
        }
        if !e.undo.is_empty() {
            undo.push(Op::Ps(e.undo.join("\n")));
        }
        if apply.is_empty() {
            continue;
        }
        v.push(Tweak {
            name: e.content.replace(" - Disable", ": off").replace(" - Enable", ": on").replace(" - Remove", ": remove"),
            id,
            category: cat.into(),
            desc: e.description,
            risk,
            source: "winutil",
            link: (!e.link.is_empty()).then_some(e.link),
            apply,
            undo,
            restart: false,
            hint: None,
        });
    }
    v
}

/// Optional Windows features (WSL, Hyper-V, Sandbox, .NET 3.5...).
pub fn features() -> Vec<Tweak> {
    let map: BTreeMap<String, Entry> = serde_json::from_str(&lenient(FEATURES)).unwrap_or_default();
    let mut v = Vec::new();
    for (id, e) in map {
        if e.category != "Features" || (e.feature.is_empty() && e.invoke.is_empty()) || e.content.contains("Install Features") {
            continue;
        }
        let names = e.feature.iter().map(|f| crate::util::ps_quote(f)).collect::<Vec<_>>().join(",");
        let mut apply = Vec::new();
        let mut undo = Vec::new();
        if !e.feature.is_empty() {
            apply.push(Op::Ps(format!("foreach($f in @({names})){{ Enable-WindowsOptionalFeature -Online -FeatureName $f -All -NoRestart -ErrorAction SilentlyContinue | Out-Null }}")));
            undo.push(Op::Ps(format!("foreach($f in @({names})){{ Disable-WindowsOptionalFeature -Online -FeatureName $f -NoRestart -ErrorAction SilentlyContinue | Out-Null }}")));
        }
        if !e.invoke.is_empty() {
            apply.push(Op::Ps(e.invoke.join("\n")));
        }
        v.push(Tweak {
            name: e.content.replace(" - Enable", ""),
            id,
            category: "Windows features".into(),
            desc: e.description,
            risk: Risk::Moderate,
            source: "winutil",
            link: (!e.link.is_empty()).then_some(e.link),
            apply,
            undo,
            restart: true,
            hint: None,
        });
    }
    v
}

pub fn presets() -> Vec<(&'static str, &'static str, Vec<String>)> {
    let map: BTreeMap<String, Vec<String>> = serde_json::from_str(&lenient(PRESETS)).unwrap_or_default();
    let mut v = Vec::new();
    for (name, desc) in
        [("Standard", "winutil's recommended set"), ("Minimal", "winutil's minimal set"), ("Advanced", "winutil's advanced set")]
    {
        if let Some(ids) = map.get(name) {
            let label: &'static str = match name {
                "Standard" => "winutil Standard",
                "Minimal" => "winutil Minimal",
                _ => "winutil Advanced",
            };
            v.push((label, desc, ids.clone()));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_embedded_catalog() {
        let t = super::tweaks();
        assert!(t.len() > 40, "only {} winutil tweaks parsed", t.len());
        assert!(t.iter().any(|x| x.id == "WPFTweaksTelemetry"));
        assert!(!super::features().is_empty());
        assert!(!super::presets().is_empty());
    }
}
