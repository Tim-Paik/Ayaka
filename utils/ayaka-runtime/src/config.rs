#[doc(no_inline)]
pub use ayaka_bindings_types::{Action, Switch};
#[doc(no_inline)]
pub use fallback::Fallback;

use crate::*;
use serde::{de::DeserializeOwned, Deserialize};
use std::{collections::HashMap, ops::Deref, path::PathBuf, sync::OnceLock};

/// The paragraph in a paragraph config.
#[derive(Debug, Deserialize)]
pub struct Paragraph {
    /// The tag and key of a paragraph.
    /// They are referenced in `next`.
    pub tag: String,
    /// The title of a paragraph.
    /// It can be [`None`], but better with a human-readable one.
    pub title: Option<String>,
    /// The texts.
    /// They will be parsed into [`ayaka_script_types::Text`] later.
    pub texts: Vec<String>,
    /// The next paragraph.
    /// If [`None`], the game meets the end.
    pub next: Option<String>,
}

/// The Ayaka config.
/// It should be deserialized from a YAML file.
#[derive(Debug, Default, Deserialize)]
pub struct GameConfig {
    /// The title of the game.
    pub title: String,
    /// The author of the game.
    #[serde(default)]
    pub author: String,
    /// The paragraphs path.
    pub paras: PathBuf,
    /// The start paragraph tag.
    pub start: String,
    /// The plugin config.
    #[serde(default)]
    pub plugins: PluginConfig,
    /// The global game properties.
    #[serde(default)]
    pub props: HashMap<String, String>,
    /// The resources path.
    pub res: Option<PathBuf>,
    /// The base language.
    /// If the runtime fails to choose a best match,
    /// it fallbacks to this one.
    pub base_lang: Locale,
}

/// The plugin config.
#[derive(Debug, Default, Deserialize)]
pub struct PluginConfig {
    /// The directory of the plugins.
    pub dir: PathBuf,
    /// The names of the plugins, without extension.
    #[serde(default)]
    pub modules: Vec<String>,
}

/// The full Ayaka game.
/// It consists of global config and all paragraphs.
pub struct Game {
    /// The game config.
    pub config: GameConfig,
    /// The paragraphs, indexed by locale.
    /// The inner is the paragraphs indexed by file names.
    pub paras: HashMap<Locale, HashMap<String, LoadLock<Vec<Paragraph>>>>,
    /// The resources, indexed by locale.
    pub res: HashMap<Locale, LoadLock<VarMap>>,
}

impl Game {
    fn choose_from_keys<'a, V>(&'a self, loc: &Locale, map: &'a HashMap<Locale, V>) -> &'a Locale {
        loc.choose_from(map.keys())
            .unwrap_or(&self.config.base_lang)
    }

    fn find_para(&self, loc: &Locale, base_tag: &str, tag: &str) -> Option<&Paragraph> {
        if let Some(paras) = self.paras.get(loc) {
            if let Some(paras) = paras.get(base_tag) {
                for p in paras.iter() {
                    if p.tag == tag {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    /// Find a paragraph by tag, with specified locale.
    pub fn find_para_fallback(
        &self,
        loc: &Locale,
        base_tag: &str,
        tag: &str,
    ) -> Fallback<&Paragraph> {
        let key = self.choose_from_keys(loc, &self.paras);
        let base_key = self.choose_from_keys(&self.config.base_lang, &self.paras);
        Fallback::new(
            if key == base_key {
                None
            } else {
                self.find_para(key, base_tag, tag)
            },
            self.find_para(base_key, base_tag, tag),
        )
    }

    fn find_res(&self, loc: &Locale) -> Option<&VarMap> {
        self.res.get(loc).map(|map| map.deref())
    }

    /// Find the resource map with specified locale.
    pub fn find_res_fallback(&self, loc: &Locale) -> Fallback<&VarMap> {
        let key = self.choose_from_keys(loc, &self.res);
        let base_key = self.choose_from_keys(&self.config.base_lang, &self.res);
        Fallback::new(
            if key == base_key {
                None
            } else {
                self.find_res(key)
            },
            self.find_res(base_key),
        )
    }

    pub(crate) fn force(&self, loc: &Locale) {
        let key = self.choose_from_keys(loc, &self.paras);
        for para in self.paras[key].values() {
            para.force();
        }
        let key = self.choose_from_keys(loc, &self.res);
        self.res[key].force();
    }
}

/// A lazy loaded config or resource file.
pub struct LoadLock<T> {
    inner: OnceLock<T>,
    path: PathBuf,
}

impl<T> LoadLock<T> {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            inner: OnceLock::new(),
            path,
        }
    }
}

impl<T: DeserializeOwned> LoadLock<T> {
    pub(crate) fn force(&self) -> &T {
        self.inner.get_or_init(|| {
            let data = std::fs::read(&self.path).unwrap();
            serde_yaml::from_slice::<T>(&data).unwrap()
        })
    }
}

impl<T: DeserializeOwned> Deref for LoadLock<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.force()
    }
}
