//! A file browser for Save, Open, Import and Export (D030): the folder's
//! entries with folders first, files filtered to the wanted extension, a
//! path you can type or climb, and a name field. `std::fs` does the work.

use std::path::{Path, PathBuf};

use prism_math::{Rect, Vec2};

use crate::ui::{FILL, Ui};

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    name: String,
    is_dir: bool,
}

#[derive(Clone, Debug)]
pub struct FileBrowser {
    dir: PathBuf,
    /// The path field as typed; follows `dir` when you navigate.
    dir_text: String,
    pub name: String,
    /// Wanted extension without the dot ("prism", "obj"); empty shows all files.
    ext: String,
    pub save: bool,
    entries: Vec<Entry>,
    error: Option<String>,
}

/// What the dialog decided this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Open,
    Confirm(PathBuf),
    Cancel,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

impl FileBrowser {
    /// Start in `suggest`'s folder (else home) with its name filled in and
    /// its extension as the filter.
    pub fn new(suggest: &Path, save: bool) -> Self {
        let name = suggest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = suggest.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        let parent = suggest.parent().filter(|p| !p.as_os_str().is_empty() && p.is_dir()).map(Path::to_path_buf);
        let mut fb = Self { dir: parent.unwrap_or_else(home), dir_text: String::new(), name, ext, save, entries: Vec::new(), error: None };
        fb.refresh();
        fb
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn refresh(&mut self) {
        self.dir_text = self.dir.display().to_string();
        self.entries.clear();
        self.error = None;
        let want = format!(".{}", self.ext.to_lowercase());
        match std::fs::read_dir(&self.dir) {
            Ok(read) => {
                for e in read.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    let is_dir = e.path().is_dir();
                    if !is_dir && !self.ext.is_empty() && !name.to_lowercase().ends_with(&want) {
                        continue;
                    }
                    self.entries.push(Entry { name, is_dir });
                }
                self.entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn enter(&mut self, sub: &str) {
        self.dir = self.dir.join(sub);
        self.refresh();
    }

    pub fn up(&mut self) {
        if let Some(p) = self.dir.parent() {
            self.dir = p.to_path_buf();
            self.refresh();
        }
    }

    /// Jump to a typed path (`~` means home). A file path selects that file.
    pub fn go(&mut self, typed: &str) {
        let t = typed.trim();
        let path = match t.strip_prefix('~') {
            Some(rest) => home().join(rest.trim_start_matches('/')),
            None => PathBuf::from(t),
        };
        if path.is_dir() {
            self.dir = path;
            self.refresh();
        } else if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
            && parent.is_dir()
        {
            self.dir = parent.to_path_buf();
            self.name = name.to_string_lossy().into_owned();
            self.refresh();
        } else {
            self.error = Some(format!("no such folder: {t}"));
            self.dir_text = self.dir.display().to_string();
        }
    }

    /// The file the dialog stands for, with the extension added if missing.
    pub fn chosen(&self) -> Option<PathBuf> {
        let name = self.name.trim();
        if name.is_empty() {
            return None;
        }
        let mut path = self.dir.join(name);
        if path.extension().is_none() && !self.ext.is_empty() {
            path.set_extension(&self.ext);
        }
        Some(path)
    }
}

/// Lay the browser out inside `rect`; the caller has set the cursor to its
/// top-left. Folders open on a click; a file fills the name on a click and
/// confirms on a double click or Enter.
pub fn draw(ui: &mut Ui, fb: &mut FileBrowser, rect: Rect) -> Verdict {
    let m = ui.m;
    let mut verdict = Verdict::Open;
    let what = if fb.ext.is_empty() { "all files".to_owned() } else { format!(".{} files", fb.ext) };
    ui.label_dim(&format!("{} · {what}", if fb.save { "Save as" } else { "Open" }));

    // Where: home, up, and the path itself.
    ui.row(|ui| {
        if ui.button("Home").clicked {
            fb.dir = home();
            fb.refresh();
        }
        if ui.button("Up").clicked {
            fb.up();
        }
        let id = ui.id("dir");
        let r = ui.alloc(Vec2::new(FILL, m.widget_h));
        if ui.text_edit_core(id, r, &mut fb.dir_text).committed {
            let typed = fb.dir_text.clone();
            fb.go(&typed);
        }
    });
    if let Some(e) = fb.error.clone() {
        ui.label_dim(&format!("! {e}"));
    }

    // The listing takes everything but the name row.
    let list_h = (rect.max.y - ui.cursor().y - m.widget_h - m.gap * 3.0).max(m.widget_h);
    let mut enter: Option<String> = None;
    let mut pick: Option<(String, bool)> = None;
    ui.scroll_area("entries", Some(list_h), |ui| {
        if fb.entries.is_empty() && fb.error.is_none() {
            ui.label_dim("Empty folder");
        }
        for (i, e) in fb.entries.iter().enumerate() {
            ui.push_index(i);
            let label = if e.is_dir { format!("▸  {}/", e.name) } else { format!("    {}", e.name) };
            let r = ui.selectable(&label, !e.is_dir && e.name == fb.name);
            if e.is_dir {
                if r.clicked {
                    enter = Some(e.name.clone());
                }
            } else if r.double_clicked {
                pick = Some((e.name.clone(), true));
            } else if r.clicked {
                pick = Some((e.name.clone(), false));
            }
            ui.pop_id();
        }
    });
    if let Some(sub) = enter {
        fb.enter(&sub);
    }
    if let Some((name, confirm)) = pick {
        fb.name = name;
        if confirm && let Some(p) = fb.chosen() {
            verdict = Verdict::Confirm(p);
        }
    }

    // The name row: field, then the verb and Cancel.
    let verb = if fb.save { "Save" } else { "Open" };
    let style = ui.text_style();
    let buttons_w = ui.measure(verb, &style) + ui.measure("Cancel", &style) + m.pad * 4.0 + m.gap * 2.0;
    let mut go = false;
    ui.row(|ui| {
        ui.label_dim("Name");
        let id = ui.id("name");
        if ui.state.focus.is_none() {
            ui.state.focus = Some(id);
        }
        let w = (ui.avail_width() - buttons_w).max(m.px(200.0));
        let r = ui.alloc(Vec2::new(w, m.widget_h));
        go |= ui.text_edit_core(id, r, &mut fb.name).committed;
        if ui.button(verb).clicked {
            go = true;
        }
        if ui.button("Cancel").clicked {
            verdict = Verdict::Cancel;
        }
    });
    if go && let Some(p) = fb.chosen() {
        verdict = Verdict::Confirm(p);
    }
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_first_filters_by_extension_and_completes_names() {
        let dir = std::env::temp_dir().join(format!("prism-fb-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("zeta")).unwrap();
        std::fs::create_dir_all(dir.join("Alpha")).unwrap();
        std::fs::write(dir.join("b.prism"), b"x").unwrap();
        std::fs::write(dir.join("a.PRISM"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        std::fs::write(dir.join(".hidden.prism"), b"x").unwrap();
        let mut fb = FileBrowser::new(&dir.join("untitled.prism"), true);
        let names: Vec<&str> = fb.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "zeta", "a.PRISM", "b.prism"], "folders first, case-insensitive, filtered, no dotfiles");
        assert_eq!(fb.chosen(), Some(dir.join("untitled.prism")));
        fb.name = "tree".into();
        assert_eq!(fb.chosen(), Some(dir.join("tree.prism")), "the extension is added");
        fb.enter("Alpha");
        assert!(fb.dir().ends_with("Alpha") && fb.entries.is_empty());
        fb.up();
        assert_eq!(fb.dir(), dir.as_path());
        fb.go(&dir.join("b.prism").display().to_string());
        assert_eq!(fb.name, "b.prism", "a typed file path selects the file");
        fb.go("/definitely/not/a/folder");
        assert!(fb.error.is_some() && fb.dir() == dir.as_path(), "a bad path is reported, not followed");
        let none = FileBrowser::new(Path::new("untitled.obj"), false);
        assert_eq!(none.dir(), home().as_path(), "a bare name starts at home");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
