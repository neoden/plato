use std::thread;
use std::time::{Duration, Instant};
use crate::device::CURRENT_DEVICE;
use crate::framebuffer::{Framebuffer, UpdateMode, Pixmap};
use crate::geom::{Rectangle, Dir, CycleDir, halves};
use crate::unit::scale_by_dpi;
use crate::font::Fonts;
use crate::input::{DeviceEvent, ButtonCode, ButtonStatus};
use crate::view::{View, Event, Hub, Bus, RenderQueue, RenderData};
use crate::view::{Id, ID_FEEDER, ViewId};
use crate::view::{SMALL_BAR_HEIGHT, THICKNESS_MEDIUM};
use crate::document::{Document, Location};
use crate::document::html::HtmlDocument;
use crate::view::common::{toggle_main_menu, toggle_battery_menu, toggle_clock_menu};
use crate::gesture::GestureEvent;
use crate::color::BLACK;
use crate::context::Context;
use crate::view::filler::Filler;
use crate::view::image::Image;
use crate::view::top_bar::TopBar;
use crate::translator;

const VIEWER_STYLESHEET: &str = "css/dictionary.css";
const USER_STYLESHEET: &str = "css/dictionary-user.css";

// Long enough for the wifi module to load and DHCP to settle.
const NETWORK_WAIT: Duration = Duration::from_secs(45);
const RETRY_DELAY: Duration = Duration::from_millis(2500);

pub struct Translation {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    doc: HtmlDocument,
    location: usize,
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn translation_to_content(translation: &translator::Translation, source: &str, target: &str) -> String {
    let source = translation.detected_source.as_deref().unwrap_or(source);
    let mut content = format!("<p class=\"info\">{} → {}</p>\n", escape(source), escape(target));
    for paragraph in translation.text.split('\n').filter(|p| !p.trim().is_empty()) {
        content.push_str(&format!("<p>{}</p>\n", escape(paragraph)));
    }
    content
}

impl Translation {
    pub fn new(rect: Rectangle, query: &str, language: &str, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) -> Translation {
        let id = ID_FEEDER.next();
        let mut children = Vec::new();
        let dpi = CURRENT_DEVICE.dpi;
        let small_height = scale_by_dpi(SMALL_BAR_HEIGHT, dpi) as i32;
        let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
        let (small_thickness, big_thickness) = halves(thickness);

        let top_bar = TopBar::new(rect![rect.min.x, rect.min.y,
                                        rect.max.x, rect.min.y + small_height - small_thickness],
                                  Event::Back,
                                  "Translation".to_string(),
                                  context);
        children.push(Box::new(top_bar) as Box<dyn View>);

        let separator = Filler::new(rect![rect.min.x, rect.min.y + small_height - small_thickness,
                                          rect.max.x, rect.min.y + small_height + big_thickness],
                                    BLACK);
        children.push(Box::new(separator) as Box<dyn View>);

        let image_rect = rect![rect.min.x, rect.min.y + small_height + big_thickness,
                               rect.max.x, rect.max.y];
        let image = Image::new(image_rect, Pixmap::new(1, 1, 1));
        children.push(Box::new(image) as Box<dyn View>);

        let settings = &context.settings.translation;
        let mut doc = HtmlDocument::new_from_memory("<p class=\"info\">Translating…</p>");
        doc.layout(image_rect.width(), image_rect.height(), settings.font_size, dpi);
        doc.set_margin_width(settings.margin_width);
        doc.set_viewer_stylesheet(VIEWER_STYLESHEET);
        doc.set_user_stylesheet(USER_STYLESHEET);

        rq.add(RenderData::new(id, rect, UpdateMode::Gui));

        let source = translator::normalize_language(language)
                                .unwrap_or_else(|| settings.source.clone());
        let target = settings.target.clone();
        let provider = settings.provider;
        let query = query.to_string();
        let hub2 = hub.clone();

        if !context.online {
            hub.send(Event::SetWifi(true)).ok();
        }

        thread::spawn(move || {
            let started = Instant::now();
            loop {
                match translator::translate(&query, &source, &target, provider) {
                    Ok(translation) => {
                        hub2.send(Event::TranslationReady(translation_to_content(&translation, &source, &target))).ok();
                        break;
                    },
                    Err(e) => {
                        if translator::is_transient(&e) && started.elapsed() < NETWORK_WAIT {
                            thread::sleep(RETRY_DELAY);
                        } else {
                            hub2.send(Event::TranslationReady(
                                format!("<p class=\"info\">Can't translate: {}.</p>", escape(&format!("{:#}", e))))).ok();
                            break;
                        }
                    },
                }
            }
        });

        let mut translation = Translation {
            id,
            rect,
            children,
            doc,
            location: 0,
        };
        translation.render_content(&mut RenderQueue::new());
        translation
    }

    fn render_content(&mut self, rq: &mut RenderQueue) {
        if let Some(image) = self.children[2].downcast_mut::<Image>() {
            if let Some((pixmap, loc)) = self.doc.pixmap(Location::Exact(self.location), 1.0, CURRENT_DEVICE.color_samples()) {
                image.update(pixmap, rq);
                self.location = loc;
            }
        }
    }

    fn go_to_neighbor(&mut self, dir: CycleDir, rq: &mut RenderQueue) -> bool {
        let location = match dir {
            CycleDir::Previous => Location::Previous(self.location),
            CycleDir::Next => Location::Next(self.location),
        };
        match self.doc.resolve_location(location) {
            Some(loc) => {
                self.location = loc;
                self.render_content(rq);
                true
            },
            None => false,
        }
    }

    fn reseed(&mut self, rq: &mut RenderQueue, context: &mut Context) {
        if let Some(top_bar) = self.child_mut(0).downcast_mut::<TopBar>() {
            top_bar.reseed(rq, context);
        }

        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Gui));
    }
}

impl View for Translation {
    fn handle_event(&mut self, evt: &Event, hub: &Hub, _bus: &mut Bus, rq: &mut RenderQueue, context: &mut Context) -> bool {
        match *evt {
            Event::TranslationReady(ref content) => {
                self.doc.update(content);
                self.location = 0;
                self.render_content(rq);
                true
            },
            Event::Page(dir) => {
                self.go_to_neighbor(dir, rq);
                true
            },
            Event::Gesture(GestureEvent::Swipe { dir, start, .. }) if self.rect.includes(start) => {
                match dir {
                    Dir::West => { self.go_to_neighbor(CycleDir::Next, rq); },
                    Dir::East => { self.go_to_neighbor(CycleDir::Previous, rq); },
                    _ => (),
                }
                true
            },
            Event::Device(DeviceEvent::Button { code, status: ButtonStatus::Released, .. }) => {
                let cd = match code {
                    ButtonCode::Backward => Some(CycleDir::Previous),
                    ButtonCode::Forward => Some(CycleDir::Next),
                    _ => None,
                };
                if let Some(cd) = cd {
                    if !self.go_to_neighbor(cd, rq) {
                        hub.send(Event::Back).ok();
                    }
                }
                true
            },
            Event::Gesture(GestureEvent::Tap(center)) if self.rect.includes(center) => {
                let half_width = self.rect.width() as i32 / 2;
                if center.x - self.rect.min.x < half_width {
                    self.go_to_neighbor(CycleDir::Previous, rq);
                } else {
                    self.go_to_neighbor(CycleDir::Next, rq);
                }
                true
            },
            Event::ToggleNear(ViewId::MainMenu, rect) => {
                toggle_main_menu(self, rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::BatteryMenu, rect) => {
                toggle_battery_menu(self, rect, None, rq, context);
                true
            },
            Event::ToggleNear(ViewId::ClockMenu, rect) => {
                toggle_clock_menu(self, rect, None, rq, context);
                true
            },
            Event::Reseed => {
                self.reseed(rq, context);
                true
            },
            Event::Gesture(GestureEvent::Cross(_)) => {
                hub.send(Event::Back).ok();
                true
            },
            _ => false,
        }
    }

    fn render(&self, _fb: &mut dyn Framebuffer, _rect: Rectangle, _fonts: &mut Fonts) {
    }

    fn resize(&mut self, rect: Rectangle, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        let dpi = CURRENT_DEVICE.dpi;
        let small_height = scale_by_dpi(SMALL_BAR_HEIGHT, dpi) as i32;
        let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
        let (small_thickness, big_thickness) = halves(thickness);

        self.children[0].resize(rect![rect.min.x, rect.min.y,
                                      rect.max.x, rect.min.y + small_height - small_thickness],
                                hub, rq, context);

        self.children[1].resize(rect![rect.min.x, rect.min.y + small_height - small_thickness,
                                      rect.max.x, rect.min.y + small_height + big_thickness],
                                hub, rq, context);

        let image_rect = rect![rect.min.x, rect.min.y + small_height + big_thickness,
                               rect.max.x, rect.max.y];
        self.doc.layout(image_rect.width(), image_rect.height(), context.settings.translation.font_size, dpi);
        self.render_content(&mut RenderQueue::new());
        self.children[2].resize(image_rect, hub, rq, context);

        for i in 3..self.children.len() {
            self.children[i].resize(rect, hub, rq, context);
        }

        self.rect = rect;
        rq.add(RenderData::new(self.id, self.rect, UpdateMode::Full));
    }

    fn rect(&self) -> &Rectangle {
        &self.rect
    }

    fn rect_mut(&mut self) -> &mut Rectangle {
        &mut self.rect
    }

    fn children(&self) -> &Vec<Box<dyn View>> {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn View>> {
        &mut self.children
    }

    fn id(&self) -> Id {
        self.id
    }
}
