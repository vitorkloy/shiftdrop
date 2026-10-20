//! Configurações persistentes (config.json na pasta de dados do app).
//! Variáveis de ambiente (SHIFTDROP_KEYS, SHIFTDROP_SCALE) ainda têm prioridade.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Teclas das rodas convert, tools, actions: "shift" | "ctrl" | "alt".
    pub keys: [String; 3],
    /// Escala da roda (1 = 290 px).
    pub scale: f64,
    /// Iniciar com o sistema.
    pub autostart: bool,
    /// Segundos com a roda escondida até descarregar o WebView (0 = nunca).
    pub unload_secs: u64,
    /// Avisar por notificação quando houver versão nova (`None` = ainda não perguntou).
    pub notify_updates: Option<bool>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            keys: ["shift".into(), "ctrl".into(), "alt".into()],
            scale: 0.75,
            autostart: true,
            unload_secs: 30,
            notify_updates: None,
        }
    }
}

static CFG: RwLock<Option<Config>> = RwLock::new(None);

pub fn path() -> PathBuf {
    crate::deps::data_root().parent().map(|p| p.join("config.json")).unwrap_or_else(|| PathBuf::from("config.json"))
}

pub fn get() -> Config {
    if let Some(c) = CFG.read().unwrap().clone() {
        return c;
    }
    let c: Config = std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    *CFG.write().unwrap() = Some(c.clone());
    c
}

pub fn save(mut c: Config) -> Result<Config, String> {
    c.scale = c.scale.clamp(0.4, 2.0);
    for k in &mut c.keys {
        *k = k.trim().to_ascii_lowercase();
        if !["shift", "ctrl", "alt"].contains(&k.as_str()) {
            return Err(format!("tecla inválida: {k}"));
        }
    }
    if c.keys[0] == c.keys[1] || c.keys[1] == c.keys[2] || c.keys[0] == c.keys[2] {
        return Err("use uma tecla diferente para cada roda".into());
    }
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    std::fs::write(&p, serde_json::to_string_pretty(&c).unwrap()).map_err(|e| e.to_string())?;
    *CFG.write().unwrap() = Some(c.clone());
    Ok(c)
}

/// Teclas efetivas (env SHIFTDROP_KEYS=convert,tools,actions tem prioridade).
pub fn keys() -> [String; 3] {
    if let Ok(v) = std::env::var("SHIFTDROP_KEYS") {
        let ks: Vec<String> = v.split(',').map(|s| s.trim().to_ascii_lowercase()).collect();
        if ks.len() == 3 {
            return [ks[0].clone(), ks[1].clone(), ks[2].clone()];
        }
    }
    get().keys
}
