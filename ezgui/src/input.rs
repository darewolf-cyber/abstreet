use crate::widgets::{Menu, Position};
use crate::{text, Canvas, Event, InputResult, Key, ScreenPt, Text, TopMenu};
use std::collections::{BTreeMap, HashMap};

// As we check for user input, record the input and the thing that would happen. This will let us
// build up some kind of OSD of possible actions.
pub struct UserInput {
    pub(crate) event: Event,
    pub(crate) event_consumed: bool,
    important_actions: Vec<(Key, String)>,
    // If two different callers both expect the same key, there's likely an unintentional conflict.
    reserved_keys: HashMap<Key, String>,

    // When context or top menus are active, most methods lie about having input.
    // TODO This is hacky, but if we consume_event in things like get_moved_mouse, then canvas
    // dragging and UI mouseover become mutex. :\
    // TODO Logically these are borrowed, but I think that requires lots of lifetime plumbing right
    // now...
    pub(crate) context_menu: ContextMenu,
    pub(crate) top_menu: Option<TopMenu>,

    // This could be from context_menu
    pub(crate) chosen_action: Option<String>,
}

pub enum ContextMenu {
    Inactive,
    Building(ScreenPt, BTreeMap<Key, String>),
    Displaying(Menu<Key>),
    Clicked(Key),
}

impl ContextMenu {
    pub fn maybe_build(self, canvas: &Canvas) -> ContextMenu {
        match self {
            ContextMenu::Building(origin, actions) => {
                if actions.is_empty() {
                    ContextMenu::Inactive
                } else {
                    ContextMenu::Displaying(Menu::new(
                        None,
                        actions
                            .into_iter()
                            .map(|(hotkey, action)| (Some(hotkey), action, hotkey))
                            .collect(),
                        false,
                        false,
                        Position::TopLeftAt(origin),
                        canvas,
                    ))
                }
            }
            _ => self,
        }
    }
}

impl UserInput {
    pub(crate) fn new(
        event: Event,
        context_menu: ContextMenu,
        mut top_menu: Option<TopMenu>,
        canvas: &mut Canvas,
    ) -> UserInput {
        let mut input = UserInput {
            event,
            event_consumed: false,
            important_actions: Vec::new(),
            context_menu,
            // Don't move it in yet!
            top_menu: None,
            reserved_keys: HashMap::new(),
            chosen_action: None,
        };

        // First things first...
        if let Event::WindowResized(width, height) = input.event {
            canvas.window_width = width;
            canvas.window_height = height;
        }

        if let Some(ref mut menu) = top_menu {
            match menu.event(&mut input, canvas) {
                // Keep going; the input hasn't been consumed.
                InputResult::Canceled => {
                    menu.valid_actions.clear();
                    input.top_menu = top_menu;
                }
                // The context menu can't coexist.
                InputResult::StillActive => {
                    menu.valid_actions.clear();
                    input.top_menu = top_menu;
                    return input;
                }
                InputResult::Done(action, _) => {
                    menu.valid_actions.clear();
                    input.chosen_action = Some(action);
                    input.top_menu = top_menu;
                    return input;
                }
            }
        }

        // Create the context menu here, even if one already existed.
        if input.right_mouse_button_pressed() {
            assert!(!input.event_consumed);
            input.event_consumed = true;
            input.context_menu =
                ContextMenu::Building(canvas.get_cursor_in_screen_space(), BTreeMap::new());
            return input;
        }
        match input.context_menu {
            ContextMenu::Inactive => {}
            ContextMenu::Displaying(ref mut menu) => {
                // Can't call consume_event() because context_menu is borrowed.
                assert!(!input.event_consumed);
                input.event_consumed = true;
                match menu.event(input.event, canvas) {
                    InputResult::Canceled => {
                        input.context_menu = ContextMenu::Inactive;
                    }
                    InputResult::StillActive => {}
                    InputResult::Done(_, hotkey) => {
                        input.context_menu = ContextMenu::Clicked(hotkey);
                    }
                }
                return input;
            }
            ContextMenu::Building(_, _) | ContextMenu::Clicked(_) => {
                panic!("UserInput::new given a ContextMenu in an impossible state");
            }
        }

        input
    }

    pub fn key_pressed(&mut self, key: Key, action: &str) -> bool {
        if self.context_menu_active() {
            return false;
        }

        self.reserve_key(key, action);

        self.important_actions.push((key, action.to_string()));

        if self.event_consumed {
            return false;
        }

        if self.event == Event::KeyPress(key) {
            self.consume_event();
            return true;
        }
        false
    }

    pub fn contextual_action(&mut self, hotkey: Key, action: &str) -> bool {
        match self.context_menu {
            ContextMenu::Inactive => {
                // If the menu's not active (the user hasn't right-clicked yet), then still allow the
                // legacy behavior of just pressing the hotkey.
                return self.unimportant_key_pressed(hotkey, &format!("CONTEXTUAL: {}", action));
            }
            ContextMenu::Building(_, ref mut actions) => {
                // The event this round was the right click, so don't check if the right keypress
                // happened.
                if let Some(prev_action) = actions.get(&hotkey) {
                    if prev_action != action {
                        panic!(
                            "Context menu uses hotkey {:?} for both {} and {}",
                            hotkey, prev_action, action
                        );
                    }
                } else {
                    actions.insert(hotkey, action.to_string());
                }
            }
            ContextMenu::Displaying(_) => {
                if self.event == Event::KeyPress(hotkey) {
                    self.context_menu = ContextMenu::Inactive;
                    return true;
                }
            }
            ContextMenu::Clicked(key) => {
                if key == hotkey {
                    self.context_menu = ContextMenu::Inactive;
                    return true;
                }
            }
        }
        false
    }

    pub fn action_chosen(&mut self, action: &str) -> bool {
        if self.chosen_action == Some(action.to_string()) {
            self.chosen_action = None;
            return true;
        }

        if let Some(ref mut menu) = self.top_menu {
            if let Some(maybe_key) = menu.actions.get(action).cloned() {
                menu.valid_actions.insert(action.to_string());
                if let Some(key) = maybe_key {
                    self.unimportant_key_pressed(key, action)
                } else {
                    false
                }
            } else {
                panic!(
                    "action_chosen(\"{}\") doesn't match actions in the TopMenu!",
                    action
                );
            }
        } else {
            panic!("action_chosen(\"{}\") without a TopMenu defined!", action);
        }
    }

    pub fn unimportant_key_pressed(&mut self, key: Key, action: &str) -> bool {
        if self.context_menu_active() {
            return false;
        }

        self.reserve_key(key, action);

        if self.event_consumed {
            return false;
        }

        if self.event == Event::KeyPress(key) {
            self.consume_event();
            return true;
        }
        false
    }

    pub fn key_released(&mut self, key: Key) -> bool {
        if self.context_menu_active() {
            return false;
        }

        if self.event_consumed {
            return false;
        }

        if self.event == Event::KeyRelease(key) {
            self.consume_event();
            return true;
        }
        false
    }

    // No consuming for these?
    pub(crate) fn left_mouse_button_pressed(&mut self) -> bool {
        if self.context_menu_active() {
            return false;
        }
        self.event == Event::LeftMouseButtonDown
    }
    pub(crate) fn left_mouse_button_released(&mut self) -> bool {
        if self.context_menu_active() {
            return false;
        }
        self.event == Event::LeftMouseButtonUp
    }
    pub(crate) fn right_mouse_button_pressed(&mut self) -> bool {
        if self.context_menu_active() {
            return false;
        }
        self.event == Event::RightMouseButtonDown
    }

    pub(crate) fn window_gained_cursor(&mut self) -> bool {
        self.event == Event::WindowGainedCursor
    }
    pub fn window_lost_cursor(&mut self) -> bool {
        self.event == Event::WindowLostCursor
    }

    pub fn get_moved_mouse(&self) -> Option<ScreenPt> {
        if self.context_menu_active() {
            return None;
        }

        if let Event::MouseMovedTo(pt) = self.event {
            return Some(pt);
        }
        None
    }

    pub(crate) fn get_mouse_scroll(&self) -> Option<f64> {
        if self.context_menu_active() {
            return None;
        }

        if let Event::MouseWheelScroll(dy) = self.event {
            return Some(dy);
        }
        None
    }

    pub fn nonblocking_is_update_event(&mut self) -> bool {
        if self.context_menu_active() {
            return false;
        }

        if self.event_consumed {
            return false;
        }

        self.event == Event::Update
    }
    pub fn use_update_event(&mut self) {
        self.consume_event();
        assert!(self.event == Event::Update)
    }

    // TODO I'm not sure this is even useful anymore
    pub(crate) fn use_event_directly(&mut self) -> Option<Event> {
        if self.event_consumed {
            return None;
        }
        self.consume_event();
        Some(self.event)
    }

    fn consume_event(&mut self) {
        assert!(!self.event_consumed);
        self.event_consumed = true;
    }

    // Just for Wizard
    pub(crate) fn has_been_consumed(&self) -> bool {
        self.event_consumed
    }

    pub fn populate_osd(&mut self, osd: &mut Text) {
        for (key, a) in self.important_actions.drain(..) {
            osd.add_line("Press ".to_string());
            osd.append(key.describe(), Some(text::HOTKEY_COLOR));
            osd.append(format!(" to {}", a), None);
        }
    }

    fn reserve_key(&mut self, key: Key, action: &str) {
        if let Some(prev_action) = self.reserved_keys.get(&key) {
            println!("both {} and {} read key {:?}", prev_action, action, key);
        }
        self.reserved_keys.insert(key, action.to_string());
    }

    fn context_menu_active(&self) -> bool {
        match self.context_menu {
            ContextMenu::Inactive => false,
            _ => true,
        }
    }
}
