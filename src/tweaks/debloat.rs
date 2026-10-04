//! Debloat: remove preinstalled Store apps. App list from winutil appx.json (MIT).
//! Removal also de-provisions the app so it isn't reinstalled for new users;
//! anything removed can be reinstalled from the Store (Store ID is shown).

use crate::util::{self, fs as bfs};

#[derive(Clone, Debug, serde::Serialize)]
pub struct BloatApp {
    pub id: String,
    pub name: String,
    pub category: String,
    pub desc: String,
    pub package: String,
    pub store_id: String,
    pub installed: bool,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Raw {
    #[serde(rename = "Content")]
    content: String,
    #[serde(rename = "Description")]
    description: String,
    #[serde(rename = "Category")]
    category: String,
    #[serde(rename = "PackageId")]
    package: String,
    #[serde(rename = "StoreId")]
    store: String,
}

/// The catalog, with `installed` resolved against the current user's packages.
pub fn list() -> Vec<BloatApp> {
    let map: std::collections::BTreeMap<String, Raw> =
        serde_json::from_str(&super::winutil::lenient(super::winutil::APPX)).unwrap_or_default();
    let all = if util::is_admin() { "-AllUsers" } else { "" };
    let installed = util::ps(&format!("Get-AppxPackage {all} -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Name -Unique"))
        .unwrap_or_default()
        .to_lowercase();
    let names: Vec<&str> = installed.lines().map(|l| l.trim()).collect();
    let mut v: Vec<BloatApp> = map
        .into_iter()
        .filter(|(_, r)| !r.package.is_empty())
        .map(|(id, r)| BloatApp {
            installed: names.iter().any(|n| *n == r.package.to_lowercase() || bfs::wildmatch(&r.package, n)),
            id,
            name: r.content,
            category: r.category,
            desc: r.description,
            package: r.package,
            store_id: r.store,
        })
        .collect();
    v.sort_by(|a, b| b.installed.cmp(&a.installed).then(a.category.cmp(&b.category)).then(a.name.cmp(&b.name)));
    v
}

pub fn remove(app: &BloatApp) -> Result<(), String> {
    if bfs::dry() {
        return Ok(());
    }
    util::log(format!("debloat remove {}", app.package));
    let pkg = util::ps_quote(&app.package);
    let script = format!(
        "Get-AppxPackage -AllUsers -Name {pkg} | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue; \
         Get-AppxPackage -Name {pkg} | Remove-AppxPackage -ErrorAction SilentlyContinue; \
         Get-AppxProvisionedPackage -Online | Where-Object DisplayName -like {pkg} | Remove-AppxProvisionedPackage -Online -AllUsers -ErrorAction SilentlyContinue | Out-Null; \
         if (Get-AppxPackage -Name {pkg}) {{ 'still' }} else {{ 'ok' }}"
    );
    let out = util::ps(&script).map_err(|e| e.to_string())?;
    if out.contains("ok") {
        Ok(())
    } else {
        Err("Windows refused to remove it (system app?)".into())
    }
}

/// Reinstall from the Microsoft Store via winget.
pub fn reinstall(app: &BloatApp) -> Result<(), String> {
    if app.store_id.is_empty() {
        return Err("no Store ID known".into());
    }
    let (c, o) = util::run_status(
        "winget",
        &["install", "--id", &app.store_id, "--source", "msstore", "--accept-package-agreements", "--accept-source-agreements", "--silent"],
    );
    if c == 0 {
        Ok(())
    } else {
        Err(o.lines().last().unwrap_or("winget failed").to_string())
    }
}
