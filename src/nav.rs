//! Where the window can go. Pages call this; they do not hold the window.

use std::rc::Rc;

use crate::categories;
use crate::i18n::tr;
use crate::model::AppId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Discover,
    Create,
    Work,
    Play,
    Develop,
    Categories,
    Updates,
    Installed,
    Category(String),
    Article(String),
    App(AppId),
    Search(String),
    Chart(Chart),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chart {
    Popular,
    New,
    /// A hub id: create, work, play, develop.
    Hub(String),
    /// One of our category ids.
    Category(String),
}

pub enum Sidebar {
    Id(&'static str),
    Keep,
    Clear,
}

impl Route {
    pub fn from_sidebar(id: &str) -> Self {
        match id {
            "create" => Self::Create,
            "work" => Self::Work,
            "play" => Self::Play,
            "develop" => Self::Develop,
            "categories" => Self::Categories,
            "updates" => Self::Updates,
            "installed" => Self::Installed,
            _ => Self::Discover,
        }
    }

    pub fn key(&self) -> String {
        match self {
            Self::Discover => "discover".into(),
            Self::Create => "create".into(),
            Self::Work => "work".into(),
            Self::Play => "play".into(),
            Self::Develop => "develop".into(),
            Self::Categories => "categories".into(),
            Self::Updates => "updates".into(),
            Self::Installed => "installed".into(),
            Self::Category(id) => format!("category:{id}"),
            Self::Article(id) => format!("article:{id}"),
            Self::App(id) => format!("app:{}", id.key()),
            Self::Search(q) => format!("search:{q}"),
            Self::Chart(chart) => format!("chart:{}", chart.key()),
        }
    }

    pub fn title(&self) -> String {
        match self {
            Self::Discover => tr("Discover"),
            Self::Create => tr("Create"),
            Self::Work => tr("Work"),
            Self::Play => tr("Play"),
            Self::Develop => tr("Develop"),
            Self::Categories => tr("Categories"),
            Self::Updates => tr("Updates"),
            Self::Installed => tr("Installed"),
            Self::Category(id) => categories::title(id),
            Self::Article(_) | Self::App(_) => String::new(),
            Self::Search(q) => q.clone(),
            Self::Chart(chart) => chart.title(),
        }
    }

    pub fn sidebar(&self) -> Sidebar {
        match self {
            Self::Discover => Sidebar::Id("discover"),
            Self::Create => Sidebar::Id("create"),
            Self::Work => Sidebar::Id("work"),
            Self::Play => Sidebar::Id("play"),
            Self::Develop => Sidebar::Id("develop"),
            Self::Categories | Self::Category(_) => Sidebar::Id("categories"),
            Self::Updates => Sidebar::Id("updates"),
            Self::Installed => Sidebar::Id("installed"),
            Self::Search(_) => Sidebar::Clear,
            Self::Article(_) | Self::App(_) => Sidebar::Keep,
            Self::Chart(Chart::Popular | Chart::New) => Sidebar::Id("discover"),
            Self::Chart(Chart::Hub(id)) => Sidebar::Id(match id.as_str() {
                "create" => "create",
                "work" => "work",
                "play" => "play",
                "develop" => "develop",
                _ => "discover",
            }),
            Self::Chart(Chart::Category(_)) => Sidebar::Id("categories"),
        }
    }
}

impl Chart {
    fn key(&self) -> String {
        match self {
            Self::Popular => "popular".into(),
            Self::New => "new".into(),
            Self::Hub(id) => format!("hub:{id}"),
            Self::Category(id) => format!("category:{id}"),
        }
    }

    pub fn title(&self) -> String {
        match self {
            Self::Popular => tr("Popular"),
            Self::New => tr("New Apps"),
            Self::Hub(id) => match id.as_str() {
                "create" => tr("Create"),
                "work" => tr("Work"),
                "play" => tr("Play"),
                "develop" => tr("Develop"),
                _ => tr("Popular"),
            },
            Self::Category(id) => categories::title(id),
        }
    }
}

type Go = Rc<dyn Fn(Route)>;
type Confirm = Rc<dyn Fn(String, Box<dyn FnOnce()>)>;
type Retitle = Rc<dyn Fn(String)>;

#[derive(Clone)]
pub struct Nav {
    go: Go,
    confirm: Confirm,
    retitle: Retitle,
}

impl Nav {
    pub fn new(
        go: impl Fn(Route) + 'static,
        confirm: impl Fn(String, Box<dyn FnOnce()>) + 'static,
        retitle: impl Fn(String) + 'static,
    ) -> Self {
        Self {
            go: Rc::new(go),
            confirm: Rc::new(confirm),
            retitle: Rc::new(retitle),
        }
    }

    pub fn go(&self, route: Route) {
        (self.go)(route);
    }

    pub fn confirm(&self, text: String, yes: impl FnOnce() + 'static) {
        (self.confirm)(text, Box::new(yes));
    }

    pub fn retitle(&self, title: String) {
        (self.retitle)(title);
    }
}
