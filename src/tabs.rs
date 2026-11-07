//! Engine-independent tab identity, ordering and selection. Views are never recreated on switch.
pub const MAX_TABS: usize = 64;
pub type TabId = u64;

pub struct Tab<T> {
    pub id: TabId,
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub keep_awake: bool,
    pub content: T,
}

pub struct Tabs<T> {
    pub items: Vec<Tab<T>>,
    active: usize,
    next_id: TabId,
}

impl<T> Default for Tabs<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            active: 0,
            next_id: 1,
        }
    }
}

impl<T> Tabs<T> {
    pub fn next_id(&self) -> TabId {
        self.next_id
    }
    pub fn active_index(&self) -> usize {
        self.active
    }
    pub fn active(&self) -> Option<&Tab<T>> {
        self.items.get(self.active)
    }
    pub fn active_mut(&mut self) -> Option<&mut Tab<T>> {
        self.items.get_mut(self.active)
    }
    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab<T>> {
        self.items.iter_mut().find(|tab| tab.id == id)
    }
    pub fn push(&mut self, url: String, content: T) -> Result<TabId, &'static str> {
        if self.items.len() >= MAX_TABS {
            return Err("The 64-tab limit has been reached.");
        }
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Tab {
            id,
            url,
            title: "New tab".into(),
            pinned: false,
            keep_awake: false,
            content,
        });
        self.active = self.items.len() - 1;
        Ok(id)
    }
    pub fn select(&mut self, id: TabId) -> bool {
        if let Some(index) = self.items.iter().position(|tab| tab.id == id) {
            self.active = index;
            true
        } else {
            false
        }
    }
    pub fn pin(&mut self, id: TabId, pinned: bool) -> bool {
        let Some(index) = self.items.iter().position(|tab| tab.id == id) else {
            return false;
        };
        if self.items[index].pinned == pinned {
            return false;
        }
        let active = self.active().map(|tab| tab.id);
        let mut tab = self.items.remove(index);
        tab.pinned = pinned;
        let boundary = self.items.iter().take_while(|tab| tab.pinned).count();
        self.items.insert(boundary, tab);
        if let Some(id) = active {
            self.select(id);
        }
        true
    }
    /// Move within the pinned or ordinary group without recreating content.
    pub fn move_to(&mut self, id: TabId, to: usize) -> bool {
        let Some(from) = self.items.iter().position(|tab| tab.id == id) else {
            return false;
        };
        if to >= self.items.len() {
            return false;
        }
        let pins = self.items.iter().take_while(|tab| tab.pinned).count();
        let to = if self.items[from].pinned {
            to.min(pins.saturating_sub(1))
        } else {
            to.max(pins)
        };
        if from == to {
            return false;
        }
        let active = self.active().map(|tab| tab.id);
        let tab = self.items.remove(from);
        self.items.insert(to, tab);
        if let Some(id) = active {
            self.select(id);
        }
        true
    }
    pub fn cycle(&mut self, backwards: bool) {
        if !self.items.is_empty() {
            let len = self.items.len();
            self.active = (self.active + if backwards { len - 1 } else { 1 }) % len;
        }
    }
    pub fn close(&mut self, id: TabId) -> Option<Tab<T>> {
        let index = self.items.iter().position(|tab| tab.id == id)?;
        let tab = self.items.remove(index);
        if index < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.items.len().saturating_sub(1));
        Some(tab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_and_selection_survive_closing_and_wraparound() {
        let mut tabs = Tabs::default();
        let a = tabs.push("a".into(), 10).unwrap();
        let b = tabs.push("b".into(), 20).unwrap();
        let c = tabs.push("c".into(), 30).unwrap();
        tabs.close(a);
        assert_eq!(tabs.active().unwrap().id, c);
        tabs.cycle(false);
        assert_eq!(tabs.active().unwrap().id, b);
        tabs.cycle(true);
        assert_eq!(tabs.active().unwrap().content, 30);
        tabs.close(c);
        assert_eq!(tabs.active().unwrap().id, b);
        assert!(tabs.get_mut(c).is_none());
        tabs.close(b);
        assert!(tabs.active().is_none());
        assert!(tabs.push("d".into(), 40).unwrap() > c);
    }
    #[test]
    fn bounds_and_invalid_commands() {
        let mut tabs = Tabs::default();
        tabs.cycle(true);
        assert!(!tabs.select(900));
        for _ in 0..MAX_TABS {
            tabs.push("url".into(), ()).unwrap();
        }
        assert!(tabs.push("extra".into(), ()).is_err());
        assert!(tabs.close(900).is_none());
    }
}
