pub use gal_bindings_types::FrontendType;

use crate::{
    plugin::{LoadStatus, Runtime},
    progress_future::ProgressFuture,
    *,
};
use anyhow::{anyhow, Result};
use gal_script::{Command, Line, Loc, ParseError, Program, Text, TextParser};
use log::{error, warn};
use script::*;
use std::path::PathBuf;
use tokio::sync::watch::channel;
use tokio_stream::StreamExt;
use unicode_width::UnicodeWidthStr;

pub struct Context {
    pub game: Game,
    frontend: FrontendType,
    root_path: PathBuf,
    runtime: Runtime,
    loc: Locale,
    pub ctx: RawContext,
}

#[derive(Debug, Clone)]
pub enum OpenStatus {
    LoadProfile,
    CreateRuntime,
    LoadPlugin(String, usize, usize),
}

impl Context {
    pub fn open(
        path: impl Into<PathBuf>,
        frontend: FrontendType,
    ) -> ProgressFuture<Result<Self>, OpenStatus> {
        let (tx, rx) = channel(OpenStatus::LoadProfile);
        let path = path.into();
        let future = async move {
            let file = tokio::fs::read(&path).await?;
            let game: Game = serde_yaml::from_slice(&file)?;
            let root_path = path
                .parent()
                .ok_or_else(|| anyhow!("Cannot get parent from input path."))?;
            let runtime = Runtime::load(&game.plugins.dir, root_path, game.plugins.modules.clone());
            tokio::pin!(runtime);
            while let Some(load_status) = runtime.next().await {
                match load_status {
                    LoadStatus::CreateEngine => tx.send(OpenStatus::CreateRuntime)?,
                    LoadStatus::LoadPlugin(name, i, len) => {
                        tx.send(OpenStatus::LoadPlugin(name, i, len))?
                    }
                }
            }
            let runtime = runtime.await??;
            Ok(Self {
                game,
                frontend,
                root_path: root_path.to_path_buf(),
                runtime,
                loc: Locale::current(),
                ctx: RawContext::default(),
            })
        };
        ProgressFuture::new(future, rx)
    }

    pub fn init_new(&mut self) {
        let mut ctx = RawContext::default();
        ctx.cur_para = self
            .game
            .paras
            .get(&self.game.base_lang)
            .and_then(|paras| paras.first().map(|p| p.tag.clone()))
            .unwrap_or_else(|| {
                warn!("There is no paragraph in the game.");
                Default::default()
            });
        self.init_context(ctx)
    }

    pub fn init_context(&mut self, ctx: RawContext) {
        self.ctx = ctx;
    }

    fn table(&mut self) -> VarTable {
        VarTable::new(
            self.runtime.as_mut(),
            self.game.find_res_fallback(&self.loc),
            &mut self.ctx.locals,
        )
    }

    fn current_paragraph(&self) -> Fallback<&Paragraph> {
        self.game.find_para_fallback(&self.loc, &self.ctx.cur_para)
    }

    fn current_text(&self) -> Fallback<&String> {
        self.current_paragraph()
            .map(|p| {
                p.texts.get(self.ctx.cur_act).and_then(|s| {
                    if s.is_empty() || s == "~" {
                        None
                    } else {
                        Some(s)
                    }
                })
            })
            .flatten()
    }

    fn bg_dir(&self) -> PathBuf {
        self.root_path.join(&self.game.bgs)
    }

    fn bgm_dir(&self) -> PathBuf {
        self.root_path.join(&self.game.bgms)
    }

    fn video_dir(&self) -> PathBuf {
        self.root_path.join(&self.game.videos)
    }

    pub fn set_locale(&mut self, loc: Locale) {
        self.loc = loc;
    }

    pub fn locale(&self) -> &Locale {
        &self.loc
    }

    pub fn call(&mut self, expr: &impl Callable) -> RawValue {
        self.table().call(expr)
    }

    fn rich_error(&self, text: &str, e: &ParseError) -> String {
        use std::iter::repeat;
        const FREE_LEN: usize = 20;

        let loc = e.loc();
        let loc = Loc(
            text.floor_char_boundary(loc.0),
            text.ceil_char_boundary(loc.1),
        );
        let pre = text.floor_char_boundary(loc.0 - loc.0.min(FREE_LEN));
        let post = text.ceil_char_boundary(loc.1 + (text.len() - loc.1).min(FREE_LEN));

        let para_name = self
            .current_paragraph()
            .and_then(|p| p.title.as_ref())
            .map(|s| s.escape_default().to_string())
            .unwrap_or_default();
        let act_num = self.ctx.cur_act + 1;
        let show_code = &text[pre..post];
        let pre_code = &text[pre..loc.0];
        let error_code = &text[loc.0..loc.1];
        format!(
            "Parse error on paragraph \"{para_name}\", act {act_num}:\n    {show_code}\n    {}\n{e}\n",
            repeat(' ')
                .take(UnicodeWidthStr::width_cjk(pre_code))
                .chain(repeat('^').take(UnicodeWidthStr::width_cjk(error_code)))
                .collect::<String>(),
        )
    }

    fn exact_text(&mut self, para_title: Option<&String>, t: Text) -> Action {
        let mut lines = String::new();
        let mut chname = None;
        let mut switches = vec![];
        let mut switch_actions = vec![];
        let mut bg = None;
        let mut bgm = None;
        let mut video = None;
        for line in t.0.into_iter() {
            match line {
                Line::Str(s) => lines.push_str(&s),
                Line::Cmd(cmd) => match cmd {
                    Command::Par => lines.push('\n'),
                    Command::Character(key, alter) => {
                        chname = if alter.is_empty() {
                            // TODO: reduce allocation
                            let res_key = format!("ch_{}", key);
                            self.game
                                .find_res_fallback(&self.loc)
                                .and_then(|map| map.get(&res_key))
                                .map(|v| v.get_str().into_owned())
                        } else {
                            Some(alter)
                        }
                    }
                    Command::Exec(p) => lines.push_str(&self.call(&p).get_str()),
                    Command::Switch {
                        text,
                        action,
                        enabled,
                    } => {
                        // unwrap: when enabled is None, it means true.
                        let enabled = enabled.map(|p| self.call(&p).get_bool()).unwrap_or(true);
                        switches.push(Switch { text, enabled });
                        switch_actions.push(action);
                    }
                    Command::Bg(index) => bg = Some(index),
                    Command::Bgm(index) => bgm = Some(index),
                    Command::Video(index) => video = Some(index),
                },
            }
        }
        let bg = bg
            .map(|name| {
                ["jpg", "png"]
                    .into_iter()
                    .map(|ex| self.bg_dir().join(&name).with_extension(ex))
                    .filter(|p| p.exists())
                    .next()
            })
            .flatten()
            .and_then(|path| std::path::absolute(path).ok())
            .map(|p| p.to_string_lossy().into_owned());
        let bgm = bgm
            .map(|name| self.bgm_dir().join(name).with_extension("mp3"))
            .filter(|p| p.exists())
            .and_then(|path| std::path::absolute(path).ok())
            .map(|p| p.to_string_lossy().into_owned());
        let video = video
            .map(|name| self.video_dir().join(name).with_extension("mp4"))
            .filter(|p| p.exists())
            .and_then(|path| std::path::absolute(path).ok())
            .map(|p| p.to_string_lossy().into_owned());
        Action {
            line: lines,
            character: chname,
            para_title: para_title.cloned(),
            switches,
            bg,
            bgm,
            video,
            switch_actions,
        }
    }

    fn merge_action(&self, actions: Fallback<Action>) -> Option<Action> {
        if actions.is_some() {
            let actions = actions.spec();

            let line = actions.line.and_any().unwrap_or_default();
            let character = actions.character.flatten().and_any();
            let para_title = actions.para_title.flatten().and_any();
            let switches = actions
                .switches
                .into_iter()
                .map(|s| {
                    let s = s.spec();
                    let text = s.text.and_any().unwrap_or_default();
                    let (enabled, base_enabled) = s.enabled.unzip();
                    let enabled = base_enabled.or_else(|| enabled).unwrap_or(true);
                    Switch { text, enabled }
                })
                .collect();
            let bg = actions.bg.flatten().and_any();
            let bgm = actions.bgm.flatten().and_any();
            let video = actions.video.flatten().and_any();
            let switch_actions = actions
                .switch_actions
                .into_iter()
                .map(|act| act.map(|p| p.0).and_any().map(Program))
                .map(|p| p.unwrap_or_default())
                .collect();
            Some(Action {
                line,
                character,
                para_title,
                switches,
                bg,
                bgm,
                video,
                switch_actions,
            })
        } else {
            None
        }
    }

    fn process_action(&mut self, mut action: Action) -> Result<Action> {
        for (_, module) in &self.runtime.action_modules {
            action = module.process_action(&mut self.runtime.store, self.frontend, action)?;
        }
        if !action.line.is_empty() || action.character.is_some() {
            self.ctx.history.push(action.clone());
        }
        Ok(action)
    }

    fn parse_text_rich_error(&self, text: &str) -> Text {
        match TextParser::new(text).parse() {
            Ok(t) => t,
            Err(e) => {
                error!("{}", self.rich_error(text, &e));
                Text::default()
            }
        }
    }

    fn check_text_rich_error(&self, text: &str) -> bool {
        if let Err(e) = TextParser::new(text).parse() {
            eprintln!("{}", self.rich_error(text, &e));
            false
        } else {
            true
        }
    }

    pub fn next_run(&mut self) -> Option<Action> {
        let cur_para = self.current_paragraph();
        if cur_para.is_some() {
            let cur_text = self.current_text();
            if cur_text.is_some() {
                let text = cur_text.map(|act| self.parse_text_rich_error(act));
                let para_title = cur_para.and_then(|p| p.title.clone());
                let actions = text.map(|t| self.exact_text(para_title.as_ref(), t));
                self.ctx.cur_act += 1;
                self.merge_action(actions).map(|act| {
                    self.process_action(act).unwrap_or_else(|e| {
                        error!("Error when processing action: {}", e);
                        Action::default()
                    })
                })
            } else {
                self.ctx.cur_para = cur_para
                    .and_then(|p| p.next.as_ref())
                    .map(|next| self.parse_text_rich_error(next))
                    .map(|text| self.call(&text).get_str().into())
                    .unwrap_or_default();
                self.ctx.cur_act = 0;
                self.next_run()
            }
        } else {
            None
        }
    }

    pub fn check(&mut self) -> bool {
        let mut succeed = true;
        for (_, paras) in &self.game.paras {
            for para in paras {
                self.ctx.cur_para = para.tag.clone();
                for (index, act) in para.texts.iter().enumerate() {
                    self.ctx.cur_act = index;
                    succeed &= self.check_text_rich_error(act);
                }
                if let Some(next) = &para.next {
                    succeed &= self.check_text_rich_error(next);
                }
            }
        }
        succeed
    }
}
