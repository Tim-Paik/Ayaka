use gal_locale::Locale;
use gal_script::{
    log::{debug, trace, warn},
    Program, RawValue,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

pub type VarMap = HashMap<String, RawValue>;

#[derive(Debug, Deserialize)]
pub struct Paragraph {
    pub tag: String,
    pub title: Option<String>,
    pub texts: Vec<String>,
    pub next: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct RawContext {
    pub cur_para: String,
    pub cur_act: usize,
    pub locals: VarMap,
}

#[derive(Debug, Default, Deserialize)]
pub struct Game {
    pub title: String,
    #[serde(default)]
    pub author: String,
    pub paras: HashMap<Locale, Vec<Paragraph>>,
    #[serde(default)]
    pub plugins: PluginsConfig,
    #[serde(default)]
    pub bgs: PathBuf,
    #[serde(default)]
    pub bgms: PathBuf,
    #[serde(default)]
    pub res: HashMap<Locale, VarMap>,
    pub base_lang: Locale,
}

#[derive(Debug, Default, Deserialize)]
pub struct PluginsConfig {
    pub dir: PathBuf,
    #[serde(default)]
    pub modules: Vec<String>,
}

impl Game {
    fn choose_from_keys<V>(&self, loc: &Locale, map: &HashMap<Locale, V>) -> Locale {
        let keys = map.keys();
        debug!("Choose \"{}\" from {:?}", loc, keys);
        let res = loc
            .choose_from(keys)
            .unwrap_or_else(|e| {
                warn!("Cannot choose locale: {}", e);
                None
            })
            .unwrap_or_else(|| self.base_lang.clone());
        debug!("Chose \"{}\"", res);
        res
    }

    fn find_para(&self, loc: &Locale, tag: &str) -> Option<&Paragraph> {
        if let Some(paras) = self.paras.get(loc) {
            for p in paras {
                if p.tag == tag {
                    return Some(p);
                }
            }
        }
        None
    }

    pub fn find_para_fallback(&self, loc: &Locale, tag: &str) -> Fallback<&Paragraph> {
        let key = self.choose_from_keys(loc, &self.paras);
        let base_key = self.choose_from_keys(&self.base_lang, &self.paras);
        Fallback::new(
            if key == base_key {
                None
            } else {
                self.find_para(&key, tag)
            },
            self.find_para(&base_key, tag),
        )
    }

    fn find_res(&self, loc: &Locale) -> Option<&VarMap> {
        self.res.get(loc)
    }

    pub fn find_res_fallback(&self, loc: &Locale) -> Fallback<&VarMap> {
        let key = self.choose_from_keys(loc, &self.res);
        let base_key = self.choose_from_keys(&self.base_lang, &self.res);
        Fallback::new(
            if key == base_key {
                None
            } else {
                self.find_res(&key)
            },
            self.find_res(&base_key),
        )
    }
}

pub struct Fallback<T> {
    data: Option<T>,
    base_data: Option<T>,
}

impl<T> Fallback<T> {
    pub(crate) fn new(data: Option<T>, base_data: Option<T>) -> Self {
        Self { data, base_data }
    }

    pub fn is_some(&self) -> bool {
        self.data.is_some() || self.base_data.is_some()
    }

    pub fn as_ref(&self) -> Fallback<&T> {
        Fallback::new(self.data.as_ref(), self.base_data.as_ref())
    }

    pub fn and_then<V>(self, mut f: impl FnMut(T) -> Option<V>) -> Option<V> {
        self.data.and_then(|t| f(t)).or_else(|| {
            trace!("Fallback occurred");
            self.base_data.and_then(|t| f(t))
        })
    }

    pub fn map<V>(self, mut f: impl FnMut(T) -> V) -> Fallback<V> {
        Fallback::new(self.data.map(|t| f(t)), self.base_data.map(|t| f(t)))
    }

    pub fn unzip(self) -> (Option<T>, Option<T>) {
        (self.data, self.base_data)
    }
}

impl<T> Fallback<Option<T>> {
    pub fn flatten(self) -> Fallback<T> {
        Fallback::new(self.data.flatten(), self.base_data.flatten())
    }
}

impl Fallback<String> {
    pub fn and_any(self) -> Option<String> {
        self.and_then(|s| if s.is_empty() { None } else { Some(s) })
    }
}

impl<T> Fallback<Vec<T>> {
    pub fn and_any(self) -> Option<Vec<T>> {
        self.and_then(|s| if s.is_empty() { None } else { Some(s) })
    }
}

impl<T> IntoIterator for Fallback<Vec<T>> {
    type Item = Fallback<T>;

    type IntoIter = FallbackVecIter<<Vec<T> as IntoIterator>::IntoIter>;

    fn into_iter(self) -> Self::IntoIter {
        FallbackVecIter {
            data: self.data.unwrap_or_default().into_iter(),
            base_data: self.base_data.unwrap_or_default().into_iter(),
        }
    }
}

pub struct FallbackVecIter<A> {
    data: A,
    base_data: A,
}

impl<A: Iterator> Iterator for FallbackVecIter<A> {
    type Item = Fallback<A::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        let d = self.data.next();
        let based = self.base_data.next();
        if d.is_some() || based.is_some() {
            Some(Fallback::new(d, based))
        } else {
            None
        }
    }
}

#[derive(Debug, Default)]
pub struct Action {
    pub data: ActionData,
    pub switch_actions: Vec<Program>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ActionData {
    pub line: String,
    pub character: Option<String>,
    pub switches: Vec<Switch>,
    pub bg: Option<String>,
    pub bgm: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Switch {
    pub text: String,
    pub enabled: bool,
}

pub struct FallbackAction {
    pub data: Fallback<ActionData>,
    pub switch_actions: Fallback<Vec<Program>>,
}

impl Fallback<Action> {
    pub fn fallback(self) -> FallbackAction {
        let (act, base_act) = self.unzip();
        let (data, sactions) = match act {
            Some(act) => (Some(act.data), Some(act.switch_actions)),
            None => (None, None),
        };
        let (base_data, base_sactions) = match base_act {
            Some(act) => (Some(act.data), Some(act.switch_actions)),
            None => (None, None),
        };
        FallbackAction {
            data: Fallback::new(data, base_data),
            switch_actions: Fallback::new(sactions, base_sactions),
        }
    }
}

pub struct FallbackActionData {
    pub line: Fallback<String>,
    pub character: Fallback<String>,
    pub switches: Fallback<Vec<Switch>>,
    pub bg: Fallback<String>,
    pub bgm: Fallback<String>,
}

impl Fallback<ActionData> {
    pub fn fallback(self) -> FallbackActionData {
        let (data, base_data) = self.unzip();
        let (line, ch, sw, bg, bgm) = match data {
            Some(data) => (
                Some(data.line),
                data.character,
                Some(data.switches),
                data.bg,
                data.bgm,
            ),
            None => (None, None, None, None, None),
        };
        let (base_line, base_ch, base_sw, base_bg, base_bgm) = match base_data {
            Some(data) => (
                Some(data.line),
                data.character,
                Some(data.switches),
                data.bg,
                data.bgm,
            ),
            None => (None, None, None, None, None),
        };
        FallbackActionData {
            line: Fallback::new(line, base_line),
            character: Fallback::new(ch, base_ch),
            switches: Fallback::new(sw, base_sw),
            bg: Fallback::new(bg, base_bg),
            bgm: Fallback::new(bgm, base_bgm),
        }
    }
}

pub struct FallbackSwitch {
    pub text: Fallback<String>,
    pub enabled: Fallback<bool>,
}

impl Fallback<Switch> {
    pub fn fallback(self) -> FallbackSwitch {
        let (s, base_s) = self.unzip();
        let (text, enabled) = match s {
            Some(s) => (Some(s.text), Some(s.enabled)),
            None => (None, None),
        };
        let (base_text, base_enabled) = match base_s {
            Some(s) => (Some(s.text), Some(s.enabled)),
            None => (None, None),
        };
        FallbackSwitch {
            text: Fallback::new(text, base_text),
            enabled: Fallback::new(enabled, base_enabled),
        }
    }
}
