//! Modern GUI System for CottonOS
//!
//! Dark, minimal, modern UI with rounded corners

use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::graphics::{Color, FRAMEBUFFER, BackBuffer, swap_buffers, init_back_buffer};
use crate::drivers::mouse;
use crate::kprintln;

/// Window structure
pub struct Window {
    pub id: u32,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub visible: bool,
    pub focused: bool,
    pub dragging: bool,
    pub drag_offset_x: i32,
    pub drag_offset_y: i32,
    pub content: WindowContent,
}

/// Window content type
pub enum WindowContent {
    Empty,
    Text(String),
    Terminal(TerminalState),
    About(AboutState),
    FileManager(FileManagerState),
    TextEditor(TextEditorState),
    SaveAs(SaveAsState),
}

/// About/System Info state with scroll support
pub struct AboutState {
    pub scroll_offset: i32,
    pub max_scroll: i32,
}

impl AboutState {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            max_scroll: 150, // Total content height - visible height (will be calculated)
        }
    }
}

/// Terminal state for terminal windows
pub struct TerminalState {
    pub buffer: String,
    pub input: String,
    pub cursor_visible: bool,
    pub scroll_offset: u32,
}

/// File manager state
pub struct FileManagerState {
    pub current_path: String,
    pub files: Vec<FileEntry>,
    pub selected: Option<usize>,
    pub history: Vec<String>,
    pub history_index: usize,
    pub scroll_offset: usize,
}

/// File entry with type info
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
}

/// Modern minimal text editor state
pub struct TextEditorState {
    /// Lines of text (each line is a separate string)
    pub lines: Vec<String>,
    /// Cursor line (0-indexed)
    pub cursor_line: usize,
    /// Cursor column (0-indexed)
    pub cursor_col: usize,
    /// Vertical scroll offset (first visible line)
    pub scroll_y: usize,
    /// Horizontal scroll offset
    pub scroll_x: usize,
    /// Current filename (None if untitled)
    pub filename: Option<String>,
    /// Whether file has unsaved changes
    pub modified: bool,
    /// Undo history (stores previous states as line snapshots)
    pub undo_stack: Vec<(Vec<String>, usize, usize)>,
    /// Redo history
    pub redo_stack: Vec<(Vec<String>, usize, usize)>,
    /// Selection start (line, col) - None if no selection
    pub selection_start: Option<(usize, usize)>,
    /// Cursor blink state
    pub cursor_visible: bool,
    /// Cursor blink counter
    pub blink_counter: u32,
}

/// Save As dialog state
pub struct SaveAsState {
    pub filename: String,
    pub current_dir: String,
    pub dirs: Vec<FileEntry>,
    pub selected: Option<usize>,
    pub scroll_offset: usize,
    pub content: String,
}

impl SaveAsState {
    pub fn new(current_dir: &str, default_name: &str, content: &str) -> Self {
        let mut dirs: Vec<FileEntry> = Vec::new();
        if let Ok(entries) = crate::fs::readdir(current_dir) {
            for e in entries {
                // Skip . and .. - we handle parent navigation separately
                if e.name == "." || e.name == ".." {
                    continue;
                }
                if e.file_type == crate::fs::vfs::FileType::Directory {
                    dirs.push(FileEntry { name: e.name.clone(), is_dir: true });
                }
            }
        }
        // Sort directories alphabetically
        dirs.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            filename: String::from(default_name),
            current_dir: String::from(current_dir),
            dirs,
            selected: None,
            scroll_offset: 0,
            content: String::from(content),
        }
    }

    pub fn refresh(&mut self) {
        self.dirs.clear();
        if let Ok(entries) = crate::fs::readdir(&self.current_dir) {
            for e in entries {
                // Skip . and .. - we handle parent navigation separately
                if e.name == "." || e.name == ".." {
                    continue;
                }
                if e.file_type == crate::fs::vfs::FileType::Directory {
                    self.dirs.push(FileEntry { name: e.name.clone(), is_dir: true });
                }
            }
        }
        // Sort directories alphabetically
        self.dirs.sort_by(|a, b| a.name.cmp(&b.name));
        self.selected = None;
        self.scroll_offset = 0;
    }
}

impl TextEditorState {
    pub fn new() -> Self {
        Self {
            lines: alloc::vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
            scroll_y: 0,
            scroll_x: 0,
            filename: None,
            modified: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            selection_start: None,
            cursor_visible: true,
            blink_counter: 0,
        }
    }
    
    /// Load file content into editor
    pub fn load_file(&mut self, path: &str) {
        if let Ok(content) = crate::fs::read_file(path) {
            let text = alloc::string::String::from_utf8_lossy(&content).into_owned();
            self.lines = text.lines().map(String::from).collect();
            if self.lines.is_empty() {
                self.lines.push(String::new());
            }
            self.filename = Some(String::from(path));
            self.cursor_line = 0;
            self.cursor_col = 0;
            self.scroll_y = 0;
            self.scroll_x = 0;
            self.modified = false;
            self.undo_stack.clear();
            self.redo_stack.clear();
            self.selection_start = None;
        }
    }
    
    /// Get content as a single string
    pub fn content(&self) -> String {
        self.lines.join("\n")
    }
    
    /// Save file
    pub fn save_file(&mut self) -> bool {
        if let Some(ref path) = self.filename {
            let content = self.content();
            if crate::fs::write_file(path, content.as_bytes()).is_ok() {
                self.modified = false;
                return true;
            }
        }
        false
    }
    
    /// Save state for undo
    fn push_undo(&mut self) {
        self.undo_stack.push((self.lines.clone(), self.cursor_line, self.cursor_col));
        self.redo_stack.clear();
        // Limit undo history to 100 states
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }
    
    /// Undo last action
    pub fn undo(&mut self) {
        if let Some((lines, line, col)) = self.undo_stack.pop() {
            self.redo_stack.push((self.lines.clone(), self.cursor_line, self.cursor_col));
            self.lines = lines;
            self.cursor_line = line;
            self.cursor_col = col;
            self.modified = true;
        }
    }
    
    /// Redo last undone action
    pub fn redo(&mut self) {
        if let Some((lines, line, col)) = self.redo_stack.pop() {
            self.undo_stack.push((self.lines.clone(), self.cursor_line, self.cursor_col));
            self.lines = lines;
            self.cursor_line = line;
            self.cursor_col = col;
            self.modified = true;
        }
    }
    
    /// Insert character at cursor position
    pub fn insert_char(&mut self, c: char) {
        self.push_undo();
        self.selection_start = None;
        
        if c == '\n' {
            // Split line at cursor
            let current_line = &self.lines[self.cursor_line];
            let (before, after) = current_line.split_at(self.cursor_col.min(current_line.len()));
            let before = String::from(before);
            let after = String::from(after);
            self.lines[self.cursor_line] = before;
            self.cursor_line += 1;
            self.lines.insert(self.cursor_line, after);
            self.cursor_col = 0;
        } else {
            // Insert character in current line
            let line = &mut self.lines[self.cursor_line];
            if self.cursor_col >= line.len() {
                line.push(c);
            } else {
                line.insert(self.cursor_col, c);
            }
            self.cursor_col += 1;
        }
        self.modified = true;
    }
    
    /// Delete character before cursor (backspace)
    pub fn delete_char(&mut self) {
        self.push_undo();
        self.selection_start = None;
        
        if self.cursor_col > 0 {
            // Delete character in current line
            let line = &mut self.lines[self.cursor_line];
            if self.cursor_col <= line.len() {
                line.remove(self.cursor_col - 1);
            }
            self.cursor_col -= 1;
        } else if self.cursor_line > 0 {
            // Join with previous line
            let current = self.lines.remove(self.cursor_line);
            self.cursor_line -= 1;
            self.cursor_col = self.lines[self.cursor_line].len();
            self.lines[self.cursor_line].push_str(&current);
        }
        self.modified = true;
    }
    
    /// Delete character at cursor (delete key)
    pub fn delete_forward(&mut self) {
        self.push_undo();
        self.selection_start = None;
        
        let line = &mut self.lines[self.cursor_line];
        if self.cursor_col < line.len() {
            line.remove(self.cursor_col);
            self.modified = true;
        } else if self.cursor_line + 1 < self.lines.len() {
            // Join with next line
            let next = self.lines.remove(self.cursor_line + 1);
            self.lines[self.cursor_line].push_str(&next);
            self.modified = true;
        }
    }
    
    /// Move cursor up
    pub fn move_up(&mut self) {
        if self.cursor_line > 0 {
            self.cursor_line -= 1;
            let line_len = self.lines[self.cursor_line].len();
            if self.cursor_col > line_len {
                self.cursor_col = line_len;
            }
        }
    }
    
    /// Move cursor down
    pub fn move_down(&mut self) {
        if self.cursor_line + 1 < self.lines.len() {
            self.cursor_line += 1;
            let line_len = self.lines[self.cursor_line].len();
            if self.cursor_col > line_len {
                self.cursor_col = line_len;
            }
        }
    }
    
    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.lines[self.cursor_line].len();
        }
    }
    
    /// Move cursor right
    pub fn move_right(&mut self) {
        let line_len = self.lines[self.cursor_line].len();
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_line + 1 < self.lines.len() {
            self.cursor_line += 1;
            self.cursor_col = 0;
        }
    }
    
    /// Move cursor to start of line
    pub fn move_home(&mut self) {
        self.cursor_col = 0;
    }
    
    /// Move cursor to end of line
    pub fn move_end(&mut self) {
        self.cursor_col = self.lines[self.cursor_line].len();
    }
    
    /// Move cursor to start of file
    pub fn move_to_start(&mut self) {
        self.cursor_line = 0;
        self.cursor_col = 0;
    }
    
    /// Move cursor to end of file
    pub fn move_to_end(&mut self) {
        self.cursor_line = self.lines.len().saturating_sub(1);
        self.cursor_col = self.lines[self.cursor_line].len();
    }
    
    /// Page up
    pub fn page_up(&mut self, visible_lines: usize) {
        self.cursor_line = self.cursor_line.saturating_sub(visible_lines);
        let line_len = self.lines[self.cursor_line].len();
        if self.cursor_col > line_len {
            self.cursor_col = line_len;
        }
    }
    
    /// Page down
    pub fn page_down(&mut self, visible_lines: usize) {
        self.cursor_line = (self.cursor_line + visible_lines).min(self.lines.len().saturating_sub(1));
        let line_len = self.lines[self.cursor_line].len();
        if self.cursor_col > line_len {
            self.cursor_col = line_len;
        }
    }
    
    /// Ensure cursor is visible by adjusting scroll
    pub fn ensure_cursor_visible(&mut self, visible_lines: usize, visible_cols: usize) {
        // Vertical scroll
        if self.cursor_line < self.scroll_y {
            self.scroll_y = self.cursor_line;
        } else if self.cursor_line >= self.scroll_y + visible_lines {
            self.scroll_y = self.cursor_line - visible_lines + 1;
        }
        
        // Horizontal scroll
        if self.cursor_col < self.scroll_x {
            self.scroll_x = self.cursor_col;
        } else if self.cursor_col >= self.scroll_x + visible_cols {
            self.scroll_x = self.cursor_col - visible_cols + 1;
        }
    }
    
    /// Update cursor blink
    pub fn update_blink(&mut self) {
        self.blink_counter += 1;
        if self.blink_counter >= 30 {
            self.blink_counter = 0;
            self.cursor_visible = !self.cursor_visible;
        }
    }
    
    /// Get total line count
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
    
    /// Get total character count
    pub fn char_count(&self) -> usize {
        self.lines.iter().map(|l| l.len()).sum::<usize>() + self.lines.len().saturating_sub(1)
    }
}

impl FileManagerState {
    pub fn new(path: &str) -> Self {
        let mut state = Self {
            current_path: String::from(path),
            files: Vec::new(),
            selected: None,
            history: Vec::new(),
            history_index: 0,
            scroll_offset: 0,
        };
        state.history.push(String::from(path));
        state.refresh_files();
        state
    }
    
    pub fn refresh_files(&mut self) {
        self.files.clear();
        if let Ok(entries) = crate::fs::readdir(&self.current_path) {
            for e in entries {
                // Skip . and .. entries - we have navigation buttons for that
                if e.name == "." || e.name == ".." {
                    continue;
                }
                self.files.push(FileEntry {
                    name: e.name.clone(),
                    is_dir: e.file_type == crate::fs::vfs::FileType::Directory,
                });
            }
        }
        // Sort: directories first, then files, alphabetically
        self.files.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => core::cmp::Ordering::Less,
                (false, true) => core::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
        self.selected = None;
        self.scroll_offset = 0;
    }
    
    pub fn navigate_to(&mut self, path: &str) {
        // Truncate forward history if we navigate from middle
        if self.history_index < self.history.len() - 1 {
            self.history.truncate(self.history_index + 1);
        }
        
        self.current_path = String::from(path);
        self.history.push(String::from(path));
        self.history_index = self.history.len() - 1;
        self.refresh_files();
    }
    
    pub fn go_back(&mut self) -> bool {
        if self.history_index > 0 {
            self.history_index -= 1;
            self.current_path = self.history[self.history_index].clone();
            self.refresh_files();
            true
        } else {
            false
        }
    }
    
    pub fn go_forward(&mut self) -> bool {
        if self.history_index < self.history.len() - 1 {
            self.history_index += 1;
            self.current_path = self.history[self.history_index].clone();
            self.refresh_files();
            true
        } else {
            false
        }
    }
    
    /// Get full path of selected file (for opening in editor)
    pub fn get_selected_file_path(&self) -> Option<String> {
        if let Some(idx) = self.selected {
            if idx < self.files.len() {
                let entry = &self.files[idx];
                if !entry.is_dir {
                    let path = if self.current_path == "/" {
                        alloc::format!("/{}", entry.name)
                    } else {
                        alloc::format!("{}/{}", self.current_path, entry.name)
                    };
                    return Some(path);
                }
            }
        }
        None
    }
    
    pub fn open_selected(&mut self) -> bool {
        if let Some(idx) = self.selected {
            if idx < self.files.len() {
                let entry = &self.files[idx];
                if entry.is_dir {
                    let new_path = if self.current_path == "/" {
                        if entry.name == ".." {
                            String::from("/")
                        } else if entry.name == "." {
                            self.current_path.clone()
                        } else {
                            alloc::format!("/{}", entry.name)
                        }
                    } else {
                        if entry.name == ".." {
                            // Go up one directory
                            let mut parts: Vec<&str> = self.current_path.split('/').filter(|s| !s.is_empty()).collect();
                            parts.pop();
                            if parts.is_empty() {
                                String::from("/")
                            } else {
                                alloc::format!("/{}", parts.join("/"))
                            }
                        } else if entry.name == "." {
                            self.current_path.clone()
                        } else {
                            alloc::format!("{}/{}", self.current_path, entry.name)
                        }
                    };
                    self.navigate_to(&new_path);
                    return true;
                }
            }
        }
        false
    }
}

impl Window {
    pub fn new(id: u32, title: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            id,
            title: String::from(title),
            x,
            y,
            width,
            height,
            visible: true,
            focused: true,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
            content: WindowContent::Empty,
        }
    }
    
    /// Check if point is in title bar (32px height for modern look)
    pub fn point_in_titlebar(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.width as i32 &&
        py >= self.y && py < self.y + 32
    }
    
    /// Check if point is in close button (macOS-style)
    pub fn point_in_close(&self, px: i32, py: i32) -> bool {
        let close_x = self.x + 14;
        let close_y = self.y + 16;
        let dx = px - close_x;
        let dy = py - close_y;
        dx * dx + dy * dy <= 49  // radius 7
    }
    
    /// Check if point is in window
    pub fn point_in_window(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.width as i32 &&
        py >= self.y && py < self.y + self.height as i32
    }
}

/// Dock item for bottom dock
pub struct DockItem {
    pub name: String,
    pub action: IconAction,
}

pub enum IconAction {
    OpenTerminal,
    OpenAbout,
    OpenFiles,
    OpenEditor,
}

/// GUI state
pub struct GuiState {
    pub windows: Vec<Window>,
    pub next_window_id: u32,
    pub dock_items: Vec<DockItem>,
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub mouse_prev_left: bool,
    pub mouse_prev_right: bool,
    pub running: bool,
    pub needs_full_redraw: bool,
    pub needs_window_redraw: bool,
    pub hovered_dock: Option<usize>,
}

impl GuiState {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            next_window_id: 1,
            dock_items: Vec::new(),
            mouse_x: 640,
            mouse_y: 360,
            mouse_prev_left: false,
            mouse_prev_right: false,
            hovered_dock: None,
            running: true,
            needs_full_redraw: true,
            needs_window_redraw: false,
        }
    }
    
    /// Create a new window
    pub fn create_window(&mut self, title: &str, x: i32, y: i32, w: u32, h: u32) -> u32 {
        let id = self.next_window_id;
        self.next_window_id += 1;
        
        // Unfocus all other windows
        for win in &mut self.windows {
            win.focused = false;
        }
        
        let win = Window::new(id, title, x, y, w, h);
        self.windows.push(win);
        id
    }
    
    /// Close window by ID
    pub fn close_window(&mut self, id: u32) {
        self.windows.retain(|w| w.id != id);
    }
    
    /// Focus window
    pub fn focus_window(&mut self, id: u32) {
        for win in &mut self.windows {
            win.focused = win.id == id;
        }
        // Move to top
        if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
            let win = self.windows.remove(pos);
            self.windows.push(win);
        }
    }
}

/// Global GUI state
pub static GUI: spin::Mutex<Option<GuiState>> = spin::Mutex::new(None);

/// Initialize GUI
pub fn init() {
    let fb = FRAMEBUFFER.lock();
    if fb.address == 0 {
        kprintln!("[GUI] No framebuffer available");
        return;
    }
    
    // Initialize double buffer
    init_back_buffer(fb.width, fb.height);
    
    // Get dimensions before dropping lock
    let width = fb.width as i32;
    let height = fb.height as i32;
    drop(fb);
    
    let mut state = GuiState::new();
    
    // Set up mouse bounds
    {
        let mut m = mouse::MOUSE.lock();
        m.set_screen_size(width, height);
    }
    
    // Create dock items (macOS-style dock at bottom)
    state.dock_items.push(DockItem {
        name: String::from("Terminal"),
        action: IconAction::OpenTerminal,
    });
    
    state.dock_items.push(DockItem {
        name: String::from("Files"),
        action: IconAction::OpenFiles,
    });
    
    state.dock_items.push(DockItem {
        name: String::from("Editor"),
        action: IconAction::OpenEditor,
    });
    
    state.dock_items.push(DockItem {
        name: String::from("Info"),
        action: IconAction::OpenAbout,
    });
    
    *GUI.lock() = Some(state);
    kprintln!("[GUI] Modern GUI initialized ({}x{})", width, height);
}

/// Draw the entire desktop (everything except cursor)
pub fn draw_desktop_static() {
    let bb = BackBuffer::new();
    draw_background(&bb);
    draw_dock(&bb);
    draw_windows(&bb);
}

/// Draw background - pure black with cottonOS logo
fn draw_background(bb: &BackBuffer) {
    // Pure black background
    bb.fill_rect(0, 0, bb.width, bb.height, Color::BLACK);
    
    // Draw "cottonOS" logo in center - simple and clean
    draw_cottonos_logo(bb);
}

/// Draw the cottonOS logo using simple, clean rendering
fn draw_cottonos_logo(bb: &BackBuffer) {
    // Use a simple approach: draw "cottonOS" with the built-in font, scaled up
    let text = "cottonOS";
    let scale = 4u32; // 4x scale for larger text
    let char_w = 8 * scale;
    let char_h = 16 * scale;
    let total_w = text.len() as u32 * char_w;
    
    let x = (bb.width - total_w) / 2;
    let y = (bb.height - char_h) / 2;
    
    // Draw each character scaled
    for (i, ch) in text.chars().enumerate() {
        draw_scaled_char(bb, x + (i as u32 * char_w), y, ch, Color::WHITE, scale);
    }
}

/// Draw a character at specified scale using the built-in 8x16 font
fn draw_scaled_char(bb: &BackBuffer, x: u32, y: u32, ch: char, color: Color, scale: u32) {
    // Get character bitmap from system font
    let bitmap = get_char_bitmap(ch);
    
    for (row, &bits) in bitmap.iter().enumerate() {
        for col in 0..8u32 {
            if (bits >> (7 - col)) & 1 == 1 {
                // Draw a scaled block for each pixel
                let px = x + col * scale;
                let py = y + (row as u32) * scale;
                bb.fill_rect(px, py, scale, scale, color);
            }
        }
    }
}

/// Get bitmap for a character (8x16 font)
fn get_char_bitmap(ch: char) -> [u8; 16] {
    match ch {
        'c' => [
            0x00, 0x00, 0x00, 0x00,
            0x3C, 0x66, 0x60, 0x60,
            0x60, 0x60, 0x66, 0x3C,
            0x00, 0x00, 0x00, 0x00,
        ],
        'o' => [
            0x00, 0x00, 0x00, 0x00,
            0x3C, 0x66, 0x66, 0x66,
            0x66, 0x66, 0x66, 0x3C,
            0x00, 0x00, 0x00, 0x00,
        ],
        't' => [
            0x00, 0x00, 0x18, 0x18,
            0x7E, 0x18, 0x18, 0x18,
            0x18, 0x18, 0x18, 0x0E,
            0x00, 0x00, 0x00, 0x00,
        ],
        'n' => [
            0x00, 0x00, 0x00, 0x00,
            0x7C, 0x66, 0x66, 0x66,
            0x66, 0x66, 0x66, 0x66,
            0x00, 0x00, 0x00, 0x00,
        ],
        'O' => [
            0x00, 0x3C, 0x66, 0x66,
            0x66, 0x66, 0x66, 0x66,
            0x66, 0x66, 0x66, 0x3C,
            0x00, 0x00, 0x00, 0x00,
        ],
        'S' => [
            0x00, 0x3C, 0x66, 0x60,
            0x60, 0x3C, 0x06, 0x06,
            0x06, 0x06, 0x66, 0x3C,
            0x00, 0x00, 0x00, 0x00,
        ],
        _ => [0; 16],
    }
}

/// Redraw just the windows (no background clear - fast)
pub fn redraw_windows_only() {
    let bb = BackBuffer::new();
    draw_windows(&bb);
}

/// Draw macOS-style dock at bottom
fn draw_dock(bb: &BackBuffer) {
    let gui = GUI.lock();
    if let Some(state) = &*gui {
        let dock_item_size: u32 = 48;
        let dock_padding: u32 = 8;
        let dock_spacing: u32 = 4;
        let num_items = state.dock_items.len() as u32;
        
        let dock_width = num_items * dock_item_size + (num_items + 1) * dock_spacing + dock_padding * 2;
        let dock_height: u32 = dock_item_size + dock_padding * 2;
        let dock_x = (bb.width - dock_width) / 2;
        let dock_y = bb.height - dock_height - 8;
        
        // Dock background with frosted glass effect (dark translucent)
        bb.fill_rounded_rect(dock_x, dock_y, dock_width, dock_height, 12, Color::rgb(50, 50, 54));
        bb.draw_rounded_rect(dock_x, dock_y, dock_width, dock_height, 12, Color::rgb(80, 80, 84));
        
        // Draw dock items
        for (i, item) in state.dock_items.iter().enumerate() {
            let item_x = dock_x + dock_padding + dock_spacing + (i as u32 * (dock_item_size + dock_spacing));
            let item_y = dock_y + dock_padding;
            
            let is_hovered = state.hovered_dock == Some(i);
            let item_y = if is_hovered { item_y - 8 } else { item_y };
            
            // Draw icon background
            bb.fill_rounded_rect(item_x, item_y, dock_item_size, dock_item_size, 10, Color::rgb(72, 72, 76));
            
            // Draw icon based on type
            match &item.action {
                IconAction::OpenTerminal => {
                    // Terminal icon - minimal
                    bb.fill_rounded_rect(item_x + 8, item_y + 8, 32, 32, 4, Color::rgb(30, 30, 32));
                    bb.draw_string(item_x + 12, item_y + 18, ">_", Color::rgb(80, 250, 123), None);
                }
                IconAction::OpenFiles => {
                    // Folder icon
                    bb.fill_rounded_rect(item_x + 8, item_y + 14, 32, 24, 4, Color::rgb(100, 180, 255));
                    bb.fill_rounded_rect(item_x + 8, item_y + 10, 14, 6, 2, Color::rgb(100, 180, 255));
                }
                IconAction::OpenEditor => {
                    // Editor icon - document with lines
                    bb.fill_rounded_rect(item_x + 12, item_y + 10, 24, 30, 3, Color::rgb(200, 200, 210));
                    // Lines simulating text
                    bb.fill_rect(item_x + 16, item_y + 18, 16, 2, Color::rgb(80, 80, 90));
                    bb.fill_rect(item_x + 16, item_y + 24, 14, 2, Color::rgb(80, 80, 90));
                    bb.fill_rect(item_x + 16, item_y + 30, 12, 2, Color::rgb(80, 80, 90));
                }
                IconAction::OpenAbout => {
                    // Info icon - circle with i
                    bb.fill_circle(item_x + 24, item_y + 24, 14, Color::ACCENT);
                    bb.draw_string(item_x + 20, item_y + 17, "i", Color::WHITE, None);
                }
            }
            
            // Draw tooltip on hover
            if is_hovered {
                let tooltip_w = (item.name.len() as u32 * 8) + 16;
                let tooltip_x = item_x + dock_item_size / 2 - tooltip_w / 2;
                let tooltip_y = item_y - 28;
                bb.fill_rounded_rect(tooltip_x, tooltip_y, tooltip_w, 22, 6, Color::rgb(60, 60, 64));
                bb.draw_string(tooltip_x + 8, tooltip_y + 4, &item.name, Color::WHITE, None);
            }
        }
    }
}

/// Draw all windows
fn draw_windows(bb: &BackBuffer) {
    let gui = GUI.lock();
    if let Some(state) = &*gui {
        for window in &state.windows {
            if !window.visible { continue; }
            
            let x = window.x as u32;
            let y = window.y as u32;
            let w = window.width;
            let h = window.height;
            let radius: u32 = 10;
            
            // Window background with rounded corners
            let bg_color = if window.focused { 
                Color::rgb(44, 44, 46) 
            } else { 
                Color::rgb(38, 38, 40) 
            };
            bb.fill_rounded_rect(x, y, w, h, radius, bg_color);
            
            // Subtle border
            bb.draw_rounded_rect(x, y, w, h, radius, Color::rgb(68, 68, 70));
            
            // Title bar area (top 32px)
            let title_bg = if window.focused {
                Color::rgb(50, 50, 52)
            } else {
                Color::rgb(44, 44, 46)
            };
            // Only fill the top part for title bar effect
            bb.fill_rect(x + 1, y + 1, w - 2, 30, title_bg);
            
            // Close button only (red - macOS style)
            let btn_y = y + 10;
            bb.fill_circle(x + 14, btn_y + 6, 6, Color::CLOSE_BTN);
            
            // Title text (centered)
            let title_width = window.title.len() as u32 * 8;
            let title_x = x + (w - title_width) / 2;
            bb.draw_string(title_x, y + 8, &window.title, Color::TEXT_SECONDARY, None);
            
            // Draw window content
            draw_window_content(bb, window);
        }
    }
}

/// Draw window content
fn draw_window_content(bb: &BackBuffer, window: &Window) {
    let content_x = window.x as u32 + 1;
    let content_y = window.y as u32 + 32;
    let content_w = window.width - 2;
    let content_h = window.height - 33;
    
    match &window.content {
        WindowContent::Empty => {
            bb.fill_rect(content_x, content_y, content_w, content_h, Color::WINDOW_BG);
        }
        WindowContent::Text(text) => {
            bb.fill_rect(content_x, content_y, content_w, content_h, Color::WINDOW_BG);
            bb.draw_string(content_x + 16, content_y + 16, text, Color::TEXT_PRIMARY, None);
        }
        WindowContent::About(about_state) => {
            // System Information window with scrolling support
            let scrollbar_width: u32 = 10;
            let inner_w = content_w - scrollbar_width - 4;
            
            // Fill background
            bb.fill_rect(content_x, content_y, content_w, content_h, Color::rgb(30, 30, 32));
            
            // Layout constants
            let left_col = content_x + 12;
            let right_col = content_x + 100;
            let line_h: i32 = 18;
            let scroll_offset = about_state.scroll_offset;
            
            // Total content height calculation
            let total_content_height: i32 = 450;
            let visible_height = content_h as i32;
            let max_scroll = (total_content_height - visible_height + 20).max(0);
            
            // Base y position with scroll
            let base_y = content_y as i32 + 12 - scroll_offset;
            let mut y: i32 = base_y;
            let content_top = content_y as i32;
            let content_bottom = (content_y + content_h) as i32;
            
            // Helper macro-like function that we'll inline
            // Draw text if visible
            macro_rules! draw_text {
                ($x:expr, $y:expr, $text:expr, $color:expr) => {
                    if $y >= content_top - 16 && $y < content_bottom {
                        bb.draw_string($x, $y as u32, $text, $color, None);
                    }
                };
            }
            
            macro_rules! draw_hline_vis {
                ($x:expr, $y:expr, $w:expr, $color:expr) => {
                    if $y >= content_top && $y < content_bottom {
                        bb.draw_hline($x, $y as u32, $w, $color);
                    }
                };
            }
            
            // Header
            draw_text!(left_col, y, "System Info", Color::ACCENT);
            y += line_h + 8;
            
            // Separator
            draw_hline_vis!(left_col, y, inner_w - 24, Color::rgb(60, 60, 62));
            y += 12;
            
            // OS Info
            draw_text!(left_col, y, "OS:", Color::TEXT_SECONDARY);
            draw_text!(right_col, y, "CottonOS v0.1.0", Color::TEXT_PRIMARY);
            y += line_h;
            
            draw_text!(left_col, y, "Arch:", Color::TEXT_SECONDARY);
            draw_text!(right_col, y, "x86_64", Color::TEXT_PRIMARY);
            y += line_h;
            
            draw_text!(left_col, y, "Kernel:", Color::TEXT_SECONDARY);
            draw_text!(right_col, y, "CottonOS Kernel", Color::TEXT_PRIMARY);
            y += line_h + 8;
            
            // Separator
            draw_hline_vis!(left_col, y, inner_w - 24, Color::rgb(60, 60, 62));
            y += 12;
            
            // Memory Info
            draw_text!(left_col, y, "Memory", Color::ACCENT);
            y += line_h;
            
            let (mem_total, mem_used, mem_free) = crate::mm::physical::stats();
            let mem_total_str = alloc::format!("{} MB", mem_total / (1024 * 1024));
            let mem_free_str = alloc::format!("{} MB", mem_free / (1024 * 1024));
            let mem_used_str = alloc::format!("{} MB", mem_used / (1024 * 1024));
            
            draw_text!(left_col, y, "Total:", Color::TEXT_SECONDARY);
            draw_text!(right_col, y, &mem_total_str, Color::TEXT_PRIMARY);
            y += line_h;
            
            draw_text!(left_col, y, "Used:", Color::TEXT_SECONDARY);
            draw_text!(right_col, y, &mem_used_str, Color::TEXT_PRIMARY);
            y += line_h;
            
            draw_text!(left_col, y, "Free:", Color::TEXT_SECONDARY);
            draw_text!(right_col, y, &mem_free_str, Color::TEXT_PRIMARY);
            y += line_h + 8;
            
            // Separator
            draw_hline_vis!(left_col, y, inner_w - 24, Color::rgb(60, 60, 62));
            y += 12;
            
            // Storage Info
            draw_text!(left_col, y, "Storage", Color::ACCENT);
            y += line_h;
            
            if let Some(storage) = crate::fs::get_storage_info() {
                let total_str = storage.total_display();
                let used_str = storage.used_display();
                let free_str = storage.free_display();
                let usage_str = alloc::format!("{}%", storage.usage_percent());
                let files_str = alloc::format!("{}/{}", storage.used_inodes, storage.total_inodes);
                
                draw_text!(left_col, y, "Total:", Color::TEXT_SECONDARY);
                draw_text!(right_col, y, &total_str, Color::TEXT_PRIMARY);
                y += line_h;
                
                draw_text!(left_col, y, "Used:", Color::TEXT_SECONDARY);
                draw_text!(right_col, y, &used_str, Color::TEXT_PRIMARY);
                y += line_h;
                
                draw_text!(left_col, y, "Free:", Color::TEXT_SECONDARY);
                draw_text!(right_col, y, &free_str, Color::TEXT_PRIMARY);
                y += line_h;
                
                draw_text!(left_col, y, "Usage:", Color::TEXT_SECONDARY);
                draw_text!(right_col, y, &usage_str, Color::TEXT_PRIMARY);
                y += line_h;
                
                draw_text!(left_col, y, "Files:", Color::TEXT_SECONDARY);
                draw_text!(right_col, y, &files_str, Color::TEXT_PRIMARY);
                y += line_h;
                
                // Draw storage usage bar if visible
                y += 4;
                if y >= content_top && y + 12 < content_bottom {
                    let bar_width = inner_w - 48;
                    let bar_height = 12u32;
                    let bar_x = left_col;
                    
                    bb.fill_rounded_rect(bar_x, y as u32, bar_width, bar_height, 4, Color::rgb(50, 50, 54));
                    
                    let used_width = ((storage.usage_percent() as u32 * bar_width) / 100).min(bar_width);
                    if used_width > 0 {
                        let bar_color = if storage.usage_percent() > 90 {
                            Color::rgb(255, 80, 80)
                        } else if storage.usage_percent() > 70 {
                            Color::rgb(255, 180, 80)
                        } else {
                            Color::ACCENT
                        };
                        bb.fill_rounded_rect(bar_x, y as u32, used_width, bar_height, 4, bar_color);
                    }
                }
                y += 12 + 8;
            } else {
                draw_text!(left_col, y, "Status:", Color::TEXT_SECONDARY);
                draw_text!(right_col, y, "RAM only", Color::rgb(255, 180, 80));
                y += line_h;
            }
            
            // Separator
            y += 4;
            draw_hline_vis!(left_col, y, inner_w - 24, Color::rgb(60, 60, 62));
            y += 12;
            
            // Display Info
            draw_text!(left_col, y, "Display", Color::ACCENT);
            y += line_h;
            
            let fb = crate::drivers::graphics::FRAMEBUFFER.lock();
            let res_str = alloc::format!("{}x{}", fb.width, fb.height);
            drop(fb);
            
            draw_text!(left_col, y, "Res:", Color::TEXT_SECONDARY);
            draw_text!(right_col, y, &res_str, Color::TEXT_PRIMARY);
            y += line_h;
            
            draw_text!(left_col, y, "Color:", Color::TEXT_SECONDARY);
            draw_text!(right_col, y, "32-bit RGBA", Color::TEXT_PRIMARY);
            y += line_h + 8;
            
            // Separator
            draw_hline_vis!(left_col, y, inner_w - 24, Color::rgb(60, 60, 62));
            y += 12;
            
            // Devices
            draw_text!(left_col, y, "Devices", Color::ACCENT);
            y += line_h;
            
            draw_text!(left_col, y, "Keyboard:", Color::TEXT_SECONDARY);
            draw_text!(right_col, y, "PS/2", Color::TEXT_PRIMARY);
            y += line_h;
            
            draw_text!(left_col, y, "Mouse:", Color::TEXT_SECONDARY);
            draw_text!(right_col, y, "PS/2 + Scroll", Color::TEXT_PRIMARY);
            
            // Draw scrollbar if content exceeds visible area
            if max_scroll > 0 {
                let scrollbar_x = content_x + content_w - scrollbar_width - 2;
                let scrollbar_track_h = content_h - 8;
                let scrollbar_y = content_y + 4;
                
                // Track background
                bb.fill_rounded_rect(scrollbar_x, scrollbar_y, scrollbar_width, scrollbar_track_h, 4, Color::rgb(50, 50, 54));
                
                // Calculate thumb size and position (using integer math)
                // Clamp scroll_offset to max_scroll to prevent overflow
                let clamped_scroll = (scroll_offset as i32).min(max_scroll).max(0) as u32;
                let thumb_h = ((visible_height as u32 * scrollbar_track_h) / total_content_height as u32).max(30).min(scrollbar_track_h - 10);
                let thumb_travel = scrollbar_track_h - thumb_h;
                let thumb_y = if max_scroll > 0 {
                    scrollbar_y + ((clamped_scroll * thumb_travel) / max_scroll as u32).min(thumb_travel)
                } else {
                    scrollbar_y
                };
                
                // Thumb
                bb.fill_rounded_rect(scrollbar_x, thumb_y, scrollbar_width, thumb_h, 4, Color::rgb(100, 100, 105));
            }
        }
        WindowContent::Terminal(term) => {
            // Modern terminal - pure black
            let term_bg = Color::rgb(22, 22, 24);
            let term_fg = Color::rgb(220, 220, 220);
            let prompt_color = Color::rgb(102, 217, 239);  // Cyan prompt
            let cursor_color = Color::TEXT_PRIMARY;
            
            // Draw terminal background
            bb.fill_rect(content_x, content_y, content_w, content_h, term_bg);
            
            // Calculate text area with padding
            let text_x = content_x + 12;
            let text_y = content_y + 4;
            let text_w = content_w - 12;
            let text_h = content_h - 8;
            
            let line_height: u32 = 14;
            let char_width: u32 = 8;
            let max_chars = (text_w / char_width) as usize;
            let max_visible_lines = (text_h / line_height) as usize;
            
            // Build all display lines: buffer content + current input line
            let mut display_lines: Vec<(String, bool)> = Vec::new(); // (text, is_prompt)
            
            // Add buffer lines (previous output)
            for line in term.buffer.lines() {
                if line.is_empty() {
                    display_lines.push((String::new(), false));
                } else {
                    // Wrap long lines
                    let mut remaining = line;
                    while !remaining.is_empty() {
                        if remaining.len() <= max_chars {
                            display_lines.push((String::from(remaining), false));
                            break;
                        } else {
                            let (first, rest) = remaining.split_at(max_chars);
                            display_lines.push((String::from(first), false));
                            remaining = rest;
                        }
                    }
                }
            }
            
            // Add current input line with prompt (this is where user types)
            let prompt = alloc::format!("{}> ", crate::shell::get_cwd());
            let input_line = alloc::format!("{}{}", prompt, term.input);
            
            // Wrap input line if needed
            let mut remaining: &str = &input_line;
            let mut first_input_line = true;
            while !remaining.is_empty() {
                if remaining.len() <= max_chars {
                    display_lines.push((String::from(remaining), first_input_line));
                    break;
                } else {
                    let (first, rest) = remaining.split_at(max_chars);
                    display_lines.push((String::from(first), first_input_line));
                    remaining = rest;
                    first_input_line = false;
                }
            }
            
            // Calculate scroll position - always show bottom (most recent)
            let total_lines = display_lines.len();
            let scroll_offset = term.scroll_offset as usize;
            
            // How many lines can we show?
            let visible_count = max_visible_lines.min(total_lines);
            
            // Start from bottom, adjusted by scroll
            let end_line = if scroll_offset < total_lines {
                total_lines - scroll_offset
            } else {
                total_lines
            };
            let start_line = if end_line > visible_count {
                end_line - visible_count
            } else {
                0
            };
            
            // Draw visible lines
            for (i, idx) in (start_line..end_line).enumerate() {
                let y = text_y + (i as u32 * line_height);
                if y + line_height > content_y + content_h {
                    break;
                }
                
                let (line_text, is_prompt_line) = &display_lines[idx];
                
                if *is_prompt_line && idx == total_lines - 1 - scroll_offset.min(total_lines - 1) {
                    // This is the current input line - draw prompt in blue
                    let prompt_len = prompt.len();
                    if line_text.len() >= prompt_len {
                        bb.draw_string(text_x, y, &line_text[..prompt_len], prompt_color, Some(term_bg));
                        bb.draw_string(text_x + (prompt_len as u32 * char_width), y, &line_text[prompt_len..], term_fg, Some(term_bg));
                    } else {
                        bb.draw_string(text_x, y, line_text, prompt_color, Some(term_bg));
                    }
                } else {
                    bb.draw_string(text_x, y, line_text, term_fg, Some(term_bg));
                }
            }
            
            // Draw blinking cursor on the input line (only if not scrolled up)
            if term.cursor_visible && scroll_offset == 0 {
                // Find cursor position
                let cursor_in_input = term.input.len();
                let full_cursor_pos = prompt.len() + cursor_in_input;
                
                // Calculate which line and column the cursor is on
                let cursor_line_in_input = full_cursor_pos / max_chars;
                let cursor_col = full_cursor_pos % max_chars;
                
                // Calculate screen position
                let input_start_display_line = total_lines - 1 - cursor_line_in_input;
                if input_start_display_line >= start_line && input_start_display_line < end_line {
                    let screen_line = input_start_display_line - start_line;
                    let cursor_y = text_y + (screen_line as u32 * line_height);
                    let cursor_x = text_x + (cursor_col as u32 * char_width);
                    
                    if cursor_x < content_x + content_w - 6 {
                        bb.fill_rect(cursor_x, cursor_y, 2, 14, cursor_color);
                    }
                }
            }
            
            // Draw scroll indicator if there's more content above
            if start_line > 0 {
                bb.draw_string(content_x + content_w - 20, content_y + 4, "^", Color::TEXT_SECONDARY, Some(term_bg));
            }
        }
        WindowContent::FileManager(fm) => {
            // Modern file manager - dark theme with icon grid view
            let fm_bg = Color::rgb(30, 30, 32);
            let toolbar_bg = Color::rgb(45, 45, 48);
            let toolbar_h: u32 = 36;
            let pathbar_h: u32 = 28;
            let header_h = toolbar_h + pathbar_h;
            
            // Background
            bb.fill_rect(content_x, content_y, content_w, content_h, fm_bg);
            
            // Draw toolbar (extracted)
            draw_filemanager_toolbar(bb, content_x, content_y, content_w, fm);
            // Separator
            bb.draw_hline(content_x, content_y + toolbar_h, content_w, Color::rgb(60, 60, 62));
            
            // Icon grid area
            let grid_y = content_y + toolbar_h + 8;
            let grid_h = content_h - toolbar_h - 32; // Leave space for status bar
            
            // Icon grid settings
            let icon_size: u32 = 48;  // Icon size
            let cell_w: u32 = 90;     // Cell width
            let cell_h: u32 = 80;     // Cell height (icon + label)
            let padding: u32 = 12;
            
            let cols = ((content_w - padding * 2) / cell_w).max(1) as usize;
            let visible_rows = ((grid_h) / cell_h) as usize;
            let max_visible = cols * visible_rows;
            
            // Draw file/folder icons in grid
            let start_idx = fm.scroll_offset;
            let end_idx = (start_idx + max_visible).min(fm.files.len());
            
            for (display_i, file_idx) in (start_idx..end_idx).enumerate() {
                let file = &fm.files[file_idx];
                
                let col = display_i % cols;
                let row = display_i / cols;
                
                let cell_x = content_x + padding + (col as u32 * cell_w);
                let cell_y = grid_y + (row as u32 * cell_h);
                
                if cell_y + cell_h > content_y + content_h - 24 { break; }
                
                let is_selected = fm.selected == Some(file_idx);
                
                // Selection highlight (rounded rect around icon)
                if is_selected {
                    bb.fill_rounded_rect(cell_x + 8, cell_y, cell_w - 16, cell_h - 8, 8, Color::rgb(60, 80, 100));
                }
                
                // Center icon in cell
                let icon_x = cell_x + (cell_w - icon_size) / 2;
                let icon_y = cell_y + 4;
                
                if file.is_dir {
                    // Folder icon - larger blue folder (like macOS Finder)
                    // Folder body
                    bb.fill_rounded_rect(icon_x, icon_y + 12, icon_size, icon_size - 14, 6, Color::rgb(80, 160, 240));
                    // Folder tab
                    bb.fill_rounded_rect(icon_x, icon_y + 6, icon_size / 2, 10, 4, Color::rgb(80, 160, 240));
                    // Folder front (slightly lighter)
                    bb.fill_rounded_rect(icon_x + 2, icon_y + 16, icon_size - 4, icon_size - 22, 4, Color::rgb(100, 180, 255));
                } else {
                    // File icon - document with folded corner
                    bb.fill_rounded_rect(icon_x + 8, icon_y, icon_size - 16, icon_size, 4, Color::rgb(220, 220, 225));
                    // Folded corner
                    bb.fill_rect(icon_x + icon_size - 20, icon_y, 12, 12, Color::rgb(180, 180, 185));
                    // Lines (simulating text)
                    bb.fill_rect(icon_x + 14, icon_y + 16, icon_size - 28, 2, Color::rgb(160, 160, 165));
                    bb.fill_rect(icon_x + 14, icon_y + 22, icon_size - 28, 2, Color::rgb(160, 160, 165));
                    bb.fill_rect(icon_x + 14, icon_y + 28, icon_size - 36, 2, Color::rgb(160, 160, 165));
                }
                
                // File name (centered below icon, truncated if too long)
                let text_color = if is_selected { Color::WHITE } else { Color::TEXT_PRIMARY };
                let max_name_chars = (cell_w / 7) as usize; // Approximate chars that fit
                let display_name = if file.name.len() > max_name_chars {
                    let truncated = &file.name[..max_name_chars.saturating_sub(3)];
                    alloc::format!("{}...", truncated)
                } else {
                    file.name.clone()
                };
                let name_width = display_name.len() as u32 * 7;
                let name_x = cell_x + (cell_w - name_width) / 2;
                let name_y = cell_y + icon_size + 8;
                bb.draw_string(name_x, name_y, &display_name, text_color, None);
            }
            
            // Status bar at bottom
            let status_y = content_y + content_h - 24;
            bb.fill_rect(content_x, status_y, content_w, 24, Color::rgb(38, 38, 40));
            let status = alloc::format!("{} items", fm.files.len());
            bb.draw_string(content_x + 12, status_y + 5, &status, Color::TEXT_SECONDARY, None);
        }
        WindowContent::TextEditor(editor) => {
            // ═══════════════════════════════════════════════════════════════════
            // Modern Minimal Text Editor - Clean dark theme with proper features
            // ═══════════════════════════════════════════════════════════════════
            
            // Color scheme - dark and minimal
            let bg_color = Color::rgb(24, 24, 26);
            let text_color = Color::rgb(212, 212, 212);
            let gutter_bg = Color::rgb(30, 30, 33);
            let gutter_fg = Color::rgb(90, 90, 95);
            let gutter_active = Color::rgb(140, 140, 145);
            let toolbar_bg = Color::rgb(36, 36, 40);
            let status_bg = Color::rgb(32, 32, 36);
            let cursor_color = Color::rgb(255, 255, 255);
            let btn_save_bg = Color::rgb(70, 130, 220);
            let btn_saveas_bg = Color::rgb(60, 160, 100);
            let btn_undo_bg = Color::rgb(100, 100, 105);
            let modified_color = Color::rgb(255, 180, 80);
            
            // Layout constants
            let toolbar_h: u32 = 36;
            let status_h: u32 = 24;
            let gutter_width: u32 = 48;
            let line_height: u32 = 18;
            let char_width: u32 = 8;
            let text_padding: u32 = 8;
            
            // Calculate text area dimensions
            let text_area_y = content_y + toolbar_h;
            let text_area_h = content_h.saturating_sub(toolbar_h + status_h);
            let visible_lines = (text_area_h / line_height) as usize;
            let visible_cols = ((content_w - gutter_width - text_padding * 2) / char_width) as usize;
            
            // Fill background
            bb.fill_rect(content_x, content_y, content_w, content_h, bg_color);
            
            // ─────────────────────────────────────────────────────────────────
            // Toolbar
            // ─────────────────────────────────────────────────────────────────
            bb.fill_rect(content_x, content_y, content_w, toolbar_h, toolbar_bg);
            
            // Save button
            let btn_w: u32 = 56;
            let btn_h: u32 = 24;
            let btn_y = content_y + 6;
            let btn_spacing: u32 = 8;
            
            let save_x = content_x + 10;
            bb.fill_rounded_rect(save_x, btn_y, btn_w, btn_h, 4, btn_save_bg);
            bb.draw_string(save_x + 10, btn_y + 5, "Save", Color::WHITE, None);
            
            // Save As button
            let saveas_x = save_x + btn_w + btn_spacing;
            let saveas_w: u32 = 72;
            bb.fill_rounded_rect(saveas_x, btn_y, saveas_w, btn_h, 4, btn_saveas_bg);
            bb.draw_string(saveas_x + 8, btn_y + 5, "Save As", Color::WHITE, None);
            
            // Undo button
            let undo_x = saveas_x + saveas_w + btn_spacing;
            let undo_w: u32 = 52;
            let undo_color = if editor.undo_stack.is_empty() { Color::rgb(70, 70, 72) } else { btn_undo_bg };
            bb.fill_rounded_rect(undo_x, btn_y, undo_w, btn_h, 4, undo_color);
            bb.draw_string(undo_x + 8, btn_y + 5, "Undo", Color::rgb(180, 180, 180), None);
            
            // Redo button
            let redo_x = undo_x + undo_w + 4;
            let redo_color = if editor.redo_stack.is_empty() { Color::rgb(70, 70, 72) } else { btn_undo_bg };
            bb.fill_rounded_rect(redo_x, btn_y, undo_w, btn_h, 4, redo_color);
            bb.draw_string(redo_x + 8, btn_y + 5, "Redo", Color::rgb(180, 180, 180), None);
            
            // Filename display (right side of toolbar)
            let file_label = if let Some(ref name) = editor.filename {
                // Show just filename, not full path
                if let Some(pos) = name.rfind('/') {
                    String::from(&name[pos + 1..])
                } else {
                    name.clone()
                }
            } else {
                String::from("Untitled")
            };
            let file_x = content_x + content_w - (file_label.len() as u32 * 8) - 40;
            bb.draw_string(file_x, btn_y + 5, &file_label, Color::rgb(160, 160, 165), None);
            if editor.modified {
                bb.fill_circle(file_x - 12, btn_y + 11, 4, modified_color);
            }
            
            // Toolbar separator
            bb.draw_hline(content_x, content_y + toolbar_h - 1, content_w, Color::rgb(50, 50, 55));
            
            // ─────────────────────────────────────────────────────────────────
            // Line number gutter
            // ─────────────────────────────────────────────────────────────────
            bb.fill_rect(content_x, text_area_y, gutter_width, text_area_h, gutter_bg);
            
            // Gutter separator
            bb.fill_rect(content_x + gutter_width - 1, text_area_y, 1, text_area_h, Color::rgb(45, 45, 50));
            
            // ─────────────────────────────────────────────────────────────────
            // Text area with lines
            // ─────────────────────────────────────────────────────────────────
            let text_x = content_x + gutter_width + text_padding;
            let text_y = text_area_y + 4;
            
            let total_lines = editor.lines.len();
            let start_line = editor.scroll_y;
            let end_line = (start_line + visible_lines).min(total_lines);
            
            for (screen_row, line_idx) in (start_line..end_line).enumerate() {
                let y = text_y + (screen_row as u32 * line_height);
                
                // Line number (right-aligned in gutter)
                let line_num = line_idx + 1;
                let line_num_str = alloc::format!("{:>4}", line_num);
                let num_color = if line_idx == editor.cursor_line { gutter_active } else { gutter_fg };
                bb.draw_string(content_x + 4, y, &line_num_str, num_color, Some(gutter_bg));
                
                // Line content
                if line_idx < editor.lines.len() {
                    let line = &editor.lines[line_idx];
                    // Handle horizontal scroll
                    let display_start = editor.scroll_x.min(line.len());
                    let display_end = (display_start + visible_cols).min(line.len());
                    if display_start < line.len() {
                        let visible_text: String = line.chars().skip(display_start).take(visible_cols).collect();
                        bb.draw_string(text_x, y, &visible_text, text_color, Some(bg_color));
                    }
                }
            }
            
            // ─────────────────────────────────────────────────────────────────
            // Cursor (blinking)
            // ─────────────────────────────────────────────────────────────────
            if editor.cursor_visible && editor.cursor_line >= start_line && editor.cursor_line < end_line {
                let cursor_screen_row = editor.cursor_line - start_line;
                let cursor_screen_col = editor.cursor_col.saturating_sub(editor.scroll_x);
                
                if cursor_screen_col < visible_cols {
                    let cursor_x = text_x + (cursor_screen_col as u32 * char_width);
                    let cursor_y = text_y + (cursor_screen_row as u32 * line_height);
                    
                    // Draw cursor as thin vertical bar
                    bb.fill_rect(cursor_x, cursor_y, 2, line_height - 2, cursor_color);
                }
            }
            
            // ─────────────────────────────────────────────────────────────────
            // Status bar
            // ─────────────────────────────────────────────────────────────────
            let status_y = content_y + content_h - status_h;
            bb.fill_rect(content_x, status_y, content_w, status_h, status_bg);
            bb.draw_hline(content_x, status_y, content_w, Color::rgb(50, 50, 55));
            
            // Left: Line and column
            let pos_info = alloc::format!("Ln {}, Col {}", editor.cursor_line + 1, editor.cursor_col + 1);
            bb.draw_string(content_x + 12, status_y + 5, &pos_info, Color::rgb(140, 140, 145), None);
            
            // Center: Total lines and chars
            let file_info = alloc::format!("{} lines | {} chars", editor.line_count(), editor.char_count());
            let info_x = content_x + (content_w - file_info.len() as u32 * 8) / 2;
            bb.draw_string(info_x, status_y + 5, &file_info, Color::rgb(100, 100, 105), None);
            
            // Right: Mode/encoding indicator
            let mode_str = "UTF-8";
            let mode_x = content_x + content_w - (mode_str.len() as u32 * 8) - 12;
            bb.draw_string(mode_x, status_y + 5, mode_str, Color::rgb(100, 100, 105), None);
        }
        WindowContent::SaveAs(sas) => {
            // Save As dialog UI - Modern dark theme
            let dlg_bg = Color::rgb(36, 36, 38);
            let header_bg = Color::rgb(50, 50, 52);
            let input_bg = Color::rgb(30, 30, 32);
            let list_bg = Color::rgb(28, 28, 30);
            let selected_bg = Color::rgb(60, 90, 140);
            let folder_color = Color::rgb(100, 180, 255);
            
            bb.fill_rect(content_x, content_y, content_w, content_h, dlg_bg);

            // Header / toolbar area
            let toolbar_h = 36u32;
            bb.fill_rect(content_x, content_y, content_w, toolbar_h, header_bg);

            // Save button
            let btn_x = content_x + 12;
            let btn_y = content_y + 6;
            let btn_w = 80;
            let btn_h = 24;
            bb.fill_rounded_rect(btn_x, btn_y, btn_w, btn_h, 5, Color::rgb(100, 150, 255));
            bb.draw_string(btn_x + 18, btn_y + 6, "Save", Color::WHITE, None);

            // Cancel button
            let cancel_x = btn_x + btn_w + 12;
            bb.fill_rounded_rect(cancel_x, btn_y, btn_w, btn_h, 5, Color::rgb(120, 120, 120));
            bb.draw_string(cancel_x + 12, btn_y + 6, "Cancel", Color::WHITE, None);

            // Filename input label + box
            let input_y = content_y + toolbar_h + 12;
            bb.draw_string(content_x + 12, input_y, "Filename:", Color::TEXT_SECONDARY, None);
            let box_x = content_x + 12;
            let box_y = input_y + 18;
            let box_w = content_w - 24;
            let box_h = 28u32;
            
            // Input box with border
            bb.fill_rect(box_x, box_y, box_w, box_h, input_bg);
            bb.draw_rect(box_x, box_y, box_w, box_h, Color::rgb(70, 70, 75));
            
            // Filename text with cursor
            bb.draw_string(box_x + 8, box_y + 7, &sas.filename, Color::WHITE, None);
            // Draw cursor (blinking pipe character)
            let cursor_x = box_x + 8 + (sas.filename.len() as u32 * 8);
            bb.fill_rect(cursor_x, box_y + 5, 2, box_h - 10, Color::WHITE);

            // Current directory display
            let dir_label_y = box_y + box_h + 12;
            let dir_display = alloc::format!("Location: {}", sas.current_dir);
            bb.draw_string(content_x + 12, dir_label_y, &dir_display, Color::rgb(140, 140, 145), None);

            // Directory listing label
            let list_y = dir_label_y + 24;
            bb.draw_string(content_x + 12, list_y, "Folders:", Color::TEXT_SECONDARY, None);

            // Directory list area
            let list_x = content_x + 12;
            let list_w = content_w - 24;
            let list_top = list_y + 20;
            let list_h = content_h - (list_top - content_y) - 12;
            bb.fill_rect(list_x, list_top, list_w, list_h, list_bg);
            bb.draw_rect(list_x, list_top, list_w, list_h, Color::rgb(50, 50, 55));

            // Draw directories
            let line_h = 24u32;
            let max_lines = (list_h / line_h) as usize;
            let mut draw_index = 0usize;
            
            // Parent directory entry (go up)
            if sas.current_dir != "/" && draw_index < max_lines {
                let y = list_top + 4 + (draw_index as u32 * line_h);
                let is_sel = sas.selected == Some(usize::MAX); // Special marker for parent
                if is_sel {
                    bb.fill_rect(list_x + 2, y - 2, list_w - 4, line_h, selected_bg);
                }
                // Folder icon for parent
                bb.fill_rounded_rect(list_x + 8, y + 2, 16, 12, 2, folder_color);
                bb.fill_rounded_rect(list_x + 8, y, 8, 4, 1, folder_color);
                bb.draw_string(list_x + 30, y + 2, ".. (Parent Directory)", Color::TEXT_PRIMARY, None);
                draw_index += 1;
            }
            
            // Directory entries
            for (i, dir) in sas.dirs.iter().enumerate().skip(sas.scroll_offset).take(max_lines.saturating_sub(draw_index)) {
                let y = list_top + 4 + (draw_index as u32 * line_h);
                let is_sel = sas.selected == Some(i);
                if is_sel {
                    bb.fill_rect(list_x + 2, y - 2, list_w - 4, line_h, selected_bg);
                }
                // Folder icon
                bb.fill_rounded_rect(list_x + 8, y + 2, 16, 12, 2, folder_color);
                bb.fill_rounded_rect(list_x + 8, y, 8, 4, 1, folder_color);
                bb.draw_string(list_x + 30, y + 2, &dir.name, Color::TEXT_PRIMARY, None);
                draw_index += 1;
            }
            
            // Show message if no subdirectories
            if sas.dirs.is_empty() && sas.current_dir == "/" {
                bb.draw_string(list_x + 12, list_top + 30, "(No subdirectories)", Color::rgb(100, 100, 105), None);
            }
        }
    }
}

/// Compute a fixed path-box width clamped to available content width.
pub fn compute_path_box_width(content_w: u32) -> u32 {
    let fixed_path_w: u32 = 320;
    fixed_path_w.min(content_w.saturating_sub(48)).max(80)
}

/// Trim a path string to fit inside max_chars, showing leading ellipsis for overflow.
pub fn trim_path_for_box(path: &str, max_chars: usize) -> alloc::string::String {
    if path.len() <= max_chars { return alloc::string::String::from(path); }
    if max_chars <= 3 { return alloc::string::String::from("..."); }
    let start = path.len().saturating_sub(max_chars - 3);
    alloc::format!("...{}", &path[start..])
}

/// Draw the file manager toolbar (back/forward, action buttons, and path box)
fn draw_filemanager_toolbar(bb: &BackBuffer, content_x: u32, content_y: u32, content_w: u32, fm: &FileManagerState) {
    let toolbar_h: u32 = 36;
    let toolbar_bg = Color::rgb(45, 45, 48);
    // Toolbar background
    bb.fill_rect(content_x, content_y, content_w, toolbar_h, toolbar_bg);

    // Back button
    let back_enabled = fm.history_index > 0;
    let back_color = if back_enabled { Color::TEXT_PRIMARY } else { Color::rgb(80, 80, 82) };
    bb.fill_rounded_rect(content_x + 8, content_y + 6, 28, 24, 6, Color::rgb(60, 60, 64));
    bb.draw_string(content_x + 16, content_y + 10, "<", back_color, None);

    // Forward button
    let fwd_enabled = fm.history_index < fm.history.len().saturating_sub(1);
    let fwd_color = if fwd_enabled { Color::TEXT_PRIMARY } else { Color::rgb(80, 80, 82) };
    bb.fill_rounded_rect(content_x + 42, content_y + 6, 28, 24, 6, Color::rgb(60, 60, 64));
    bb.draw_string(content_x + 50, content_y + 10, ">", fwd_color, None);

    // Action buttons (compact)
    if let Some(idx) = fm.selected {
        if idx < fm.files.len() && !fm.files[idx].is_dir {
            let btn_w: u32 = 64; // compact width
            let btn_h: u32 = 22;
            let del_x = content_x + 86;
            let del_y = content_y + 7;
            bb.fill_rounded_rect(del_x, del_y, btn_w, btn_h, 5, Color::rgb(220, 80, 80));
            bb.draw_string(del_x + 12, del_y + 4, "Delete", Color::WHITE, None);
            let open_x = del_x + btn_w + 10;
            bb.fill_rounded_rect(open_x, del_y, btn_w, btn_h, 5, Color::rgb(100, 150, 255));
            bb.draw_string(open_x + 12, del_y + 4, "Open", Color::WHITE, None);
        }
    }

    // Path box on the right (fixed width)
    let path_box_w = compute_path_box_width(content_w);
    let path_box_h: u32 = 24;
    let path_box_x = content_x + content_w - path_box_w - 8;
    let path_box_y = content_y + 6;
    bb.fill_rounded_rect(path_box_x, path_box_y, path_box_w, path_box_h, 6, Color::rgb(60, 60, 64));
    bb.draw_rounded_rect(path_box_x, path_box_y, path_box_w, path_box_h, 6, Color::rgb(80, 80, 84));

    let max_chars = ((path_box_w - 16) / 8) as usize;
    let display_path = trim_path_for_box(&fm.current_path, max_chars);
    bb.draw_string(path_box_x + 10, path_box_y + 4, &display_path, Color::TEXT_SECONDARY, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_path_box_width_small() {
        // content too small: should clamp to min 80
        assert_eq!(compute_path_box_width(100), 80);
    }

    #[test]
    fn test_compute_path_box_width_large() {
        // content large enough: should use fixed 320
        assert_eq!(compute_path_box_width(1000), 320);
    }

    #[test]
    fn test_trim_path_for_box_short() {
        let p = "/home/user";
        assert_eq!(trim_path_for_box(p, 20), alloc::string::String::from(p));
    }

    #[test]
    fn test_trim_path_for_box_long() {
        let p = "/very/long/path/to/some/deep/directory/structure/user";
        let t = trim_path_for_box(p, 10);
        assert!(t.starts_with("..."));
        assert!(t.len() <= 10);
    }
}

/// Cursor pixel buffer - no longer needed with double buffering
/// We just redraw everything each frame

/// Draw cursor to back buffer
fn draw_cursor_to_bb(bb: &BackBuffer, mx: i32, my: i32) {
    // Cursor shape (14x21)
    let cursor: [[u8; 14]; 21] = [
        [1,1,0,0,0,0,0,0,0,0,0,0,0,0],
        [1,2,1,0,0,0,0,0,0,0,0,0,0,0],
        [1,2,2,1,0,0,0,0,0,0,0,0,0,0],
        [1,2,2,2,1,0,0,0,0,0,0,0,0,0],
        [1,2,2,2,2,1,0,0,0,0,0,0,0,0],
        [1,2,2,2,2,2,1,0,0,0,0,0,0,0],
        [1,2,2,2,2,2,2,1,0,0,0,0,0,0],
        [1,2,2,2,2,2,2,2,1,0,0,0,0,0],
        [1,2,2,2,2,2,2,2,2,1,0,0,0,0],
        [1,2,2,2,2,2,2,2,2,2,1,0,0,0],
        [1,2,2,2,2,2,2,2,2,2,2,1,0,0],
        [1,2,2,2,2,2,2,2,2,2,2,2,1,0],
        [1,2,2,2,2,2,2,1,1,1,1,1,1,1],
        [1,2,2,2,2,2,2,1,0,0,0,0,0,0],
        [1,2,2,1,2,2,2,1,0,0,0,0,0,0],
        [1,2,1,0,1,2,2,2,1,0,0,0,0,0],
        [1,1,0,0,1,2,2,2,1,0,0,0,0,0],
        [1,0,0,0,0,1,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,1,2,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,1,2,2,1,0,0,0,0],
        [0,0,0,0,0,0,1,1,1,0,0,0,0,0],
    ];
    
    for (dy, row) in cursor.iter().enumerate() {
        for (dx, &pixel) in row.iter().enumerate() {
            let px = mx + dx as i32;
            let py = my + dy as i32;
            if px >= 0 && py >= 0 && (px as u32) < bb.width && (py as u32) < bb.height {
                let color = match pixel {
                    1 => Color::BLACK,
                    2 => Color::WHITE,
                    _ => continue,
                };
                bb.set_pixel(px as u32, py as u32, color);
            }
        }
    }
}

/// Handle mouse input
pub fn handle_mouse() {
    let (mx, my) = mouse::get_position();
    let (left, right, _middle) = mouse::get_buttons();
    let scroll_delta = mouse::get_scroll_delta();
    
    let mut gui = GUI.lock();
    if let Some(state) = &mut *gui {
        let left_click = left && !state.mouse_prev_left;
        let _left_release = !left && state.mouse_prev_left;
        
        // Calculate mouse Y movement for right-click drag scrolling (trackpad workaround)
        let mouse_dy = my - state.mouse_y;
        
        // Handle right-click drag scrolling (workaround for trackpad on Mac)
        // Hold right mouse button and drag up/down to scroll
        if right && state.mouse_prev_right && mouse_dy != 0 {
            // Find window under mouse cursor and scroll it
            for window in state.windows.iter_mut().rev() {
                if window.visible && window.point_in_window(mx, my) {
                    let scroll_amount = (mouse_dy.abs() / 5).max(1);
                    match &mut window.content {
                        WindowContent::Terminal(term) => {
                            if mouse_dy < 0 {
                                // Dragging up = scroll up (show older content)
                                term.scroll_offset = term.scroll_offset.saturating_add(scroll_amount as u32);
                            } else {
                                // Dragging down = scroll down (show newer content)
                                term.scroll_offset = term.scroll_offset.saturating_sub(scroll_amount as u32);
                            }
                            state.needs_window_redraw = true;
                        }
                        WindowContent::FileManager(fm) => {
                            if mouse_dy < 0 {
                                fm.scroll_offset = fm.scroll_offset.saturating_sub(scroll_amount as usize);
                            } else {
                                let max_scroll = fm.files.len().saturating_sub(8);
                                fm.scroll_offset = (fm.scroll_offset + scroll_amount as usize).min(max_scroll);
                            }
                            state.needs_window_redraw = true;
                        }
                        WindowContent::About(about_state) => {
                            let max_scroll: i32 = 150;
                            if mouse_dy < 0 {
                                about_state.scroll_offset = (about_state.scroll_offset - scroll_amount * 3).max(0);
                            } else {
                                about_state.scroll_offset = (about_state.scroll_offset + scroll_amount * 3).min(max_scroll);
                            }
                            state.needs_window_redraw = true;
                        }
                        WindowContent::TextEditor(editor) => {
                            if mouse_dy < 0 {
                                editor.scroll_y = editor.scroll_y.saturating_sub(scroll_amount as usize);
                            } else {
                                editor.scroll_y = editor.scroll_y.saturating_add(scroll_amount as usize);
                            }
                            state.needs_window_redraw = true;
                        }
                        _ => {}
                    }
                    break;
                }
            }
        }
        
        // Handle scroll wheel - check if mouse is over a window
        if scroll_delta != 0 {
            // Find window under mouse cursor
            for window in state.windows.iter_mut().rev() {
                if window.visible && window.point_in_window(mx, my) {
                    match &mut window.content {
                        WindowContent::Terminal(term) => {
                            if scroll_delta > 0 {
                                // Scroll up (show older content)
                                term.scroll_offset = term.scroll_offset.saturating_add(3);
                            } else {
                                // Scroll down (show newer content)
                                term.scroll_offset = term.scroll_offset.saturating_sub(3);
                            }
                            state.needs_window_redraw = true;
                        }
                        WindowContent::FileManager(fm) => {
                            if scroll_delta > 0 {
                                // Scroll up
                                fm.scroll_offset = fm.scroll_offset.saturating_sub(1);
                            } else {
                                // Scroll down
                                let max_scroll = fm.files.len().saturating_sub(8);
                                fm.scroll_offset = (fm.scroll_offset + 1).min(max_scroll);
                            }
                            state.needs_window_redraw = true;
                        }
                        WindowContent::About(about_state) => {
                            let max_scroll: i32 = 150;
                            if scroll_delta > 0 {
                                // Scroll up
                                about_state.scroll_offset = (about_state.scroll_offset - 30).max(0);
                            } else {
                                // Scroll down
                                about_state.scroll_offset = (about_state.scroll_offset + 30).min(max_scroll);
                            }
                            state.needs_window_redraw = true;
                        }
                        WindowContent::TextEditor(editor) => {
                            if scroll_delta > 0 {
                                // Scroll up
                                editor.scroll_y = editor.scroll_y.saturating_sub(3);
                            } else {
                                // Scroll down
                                editor.scroll_y = editor.scroll_y.saturating_add(3);
                            }
                            state.needs_window_redraw = true;
                        }
                        _ => {}
                    }
                    break;
                }
            }
        }
        
        // Check dock hover
        let dock_item_size: i32 = 48;
        let dock_padding: i32 = 8;
        let dock_spacing: i32 = 4;
        let num_items = state.dock_items.len() as i32;
        let (bb_width, bb_height) = {
            let fb = FRAMEBUFFER.lock();
            (fb.width as i32, fb.height as i32)
        };
        
        let dock_width = num_items * dock_item_size + (num_items + 1) * dock_spacing + dock_padding * 2;
        let dock_height = dock_item_size + dock_padding * 2;
        let dock_x = (bb_width - dock_width) / 2;
        let dock_y = bb_height - dock_height - 8;
        
        let old_hovered = state.hovered_dock;
        state.hovered_dock = None;
        
        if my >= dock_y && my < dock_y + dock_height && mx >= dock_x && mx < dock_x + dock_width {
            for i in 0..state.dock_items.len() {
                let item_x = dock_x + dock_padding + dock_spacing + (i as i32 * (dock_item_size + dock_spacing));
                let item_y = dock_y + dock_padding;
                
                if mx >= item_x && mx < item_x + dock_item_size &&
                   my >= item_y - 8 && my < item_y + dock_item_size + 8 {
                    state.hovered_dock = Some(i);
                    break;
                }
            }
        }
        
        if old_hovered != state.hovered_dock {
            state.needs_full_redraw = true;
        }
        
        // Handle window dragging
        for window in state.windows.iter_mut().rev() {
            if window.dragging {
                if left {
                    window.x = mx - window.drag_offset_x;
                    window.y = my - window.drag_offset_y;
                    // Clamp position
                    if window.y < 0 { window.y = 0; }
                    state.needs_full_redraw = true;
                } else {
                    window.dragging = false;
                }
                break;
            }
        }
        
        // Handle clicks
        if left_click {
            let mut handled = false;
            
            // Check windows (reverse order = top first)
            let mut close_id: Option<u32> = None;
            let mut focus_id: Option<u32> = None;
            let mut start_drag: Option<(u32, i32, i32)> = None;
            
            for window in state.windows.iter().rev() {
                if window.point_in_close(mx, my) {
                    close_id = Some(window.id);
                    handled = true;
                    break;
                } else if window.point_in_titlebar(mx, my) {
                    focus_id = Some(window.id);
                    start_drag = Some((window.id, mx - window.x, my - window.y));
                    handled = true;
                    break;
                } else if window.point_in_window(mx, my) {
                    focus_id = Some(window.id);
                    handled = true;
                    break;
                }
            }
            
            if let Some(id) = close_id {
                state.close_window(id);
                state.needs_full_redraw = true;  // Need full redraw when closing
            } else if let Some(id) = focus_id {
                state.focus_window(id);
                state.needs_window_redraw = true;  // Just redraw windows
                if let Some((drag_id, ox, oy)) = start_drag {
                    if let Some(w) = state.windows.iter_mut().find(|w| w.id == drag_id) {
                        w.dragging = true;
                        w.drag_offset_x = ox;
                        w.drag_offset_y = oy;
                    }
                }
                
                // Handle file manager content clicks
                if let Some(w) = state.windows.iter_mut().find(|w| w.id == id && w.focused) {
                    if let WindowContent::FileManager(fm) = &mut w.content {
                        let content_x: i32 = w.x + 1;
                        let content_y: i32 = w.y + 32;
                        let content_w: i32 = (w.width as i32) - 2;
                        let content_h: i32 = (w.height as i32) - 33;
                        let toolbar_h: i32 = 36;
                        // Check toolbar button clicks
                        if my >= content_y && my < content_y + toolbar_h {
                            // Back button (x: 8-36)
                            if mx >= content_x + 8 && mx < content_x + 36 {
                                if fm.go_back() {
                                    state.needs_window_redraw = true;
                                }
                            }
                            // Forward button (x: 42-70)
                            else if mx >= content_x + 42 && mx < content_x + 70 {
                                if fm.go_forward() {
                                    state.needs_window_redraw = true;
                                }
                            }
                            // Delete/Open with Editor buttons
                            else if let Some(idx) = fm.selected {
                                if idx < fm.files.len() && !fm.files[idx].is_dir {
                                    let btn_w = 80;
                                    let btn_h = 24;
                                    let del_x = content_x + 90;
                                    let del_y = content_y + 6;
                                    // Delete
                                    if mx >= del_x && mx < del_x + btn_w && my >= del_y && my < del_y + btn_h {
                                        let file = &fm.files[idx];
                                        let path = if fm.current_path == "/" {
                                            alloc::format!("/{}", file.name)
                                        } else {
                                            alloc::format!("{}/{}", fm.current_path, file.name)
                                        };
                                        let _ = crate::fs::remove(&path);
                                        fm.refresh_files();
                                        state.needs_window_redraw = true;
                                        return;
                                    }
                                    // Open with Editor
                                    let open_x = del_x + btn_w + 12;
                                    if mx >= open_x && mx < open_x + btn_w + 20 && my >= del_y && my < del_y + btn_h {
                                        let file = &fm.files[idx];
                                        let path = if fm.current_path == "/" {
                                            alloc::format!("/{}", file.name)
                                        } else {
                                            alloc::format!("{}/{}", fm.current_path, file.name)
                                        };
                                        drop(gui);
                                        open_file_in_editor(&path);
                                        let mut gui = GUI.lock();
                                        if let Some(state) = &mut *gui {
                                            state.needs_full_redraw = true;
                                        }
                                        return;
                                    }
                                }
                            }
                        }
                        // Check grid icon clicks
                        else if my >= content_y + toolbar_h + 8 {
                            // Grid settings (must match rendering)
                            let cell_w: i32 = 90;
                            let cell_h: i32 = 80;
                            let padding: i32 = 12;
                            let grid_y = content_y + toolbar_h + 8;
                            
                            let cols = ((content_w as i32 - padding * 2) / cell_w).max(1) as usize;
                            let visible_rows = ((content_h as i32 - toolbar_h - 32) / cell_h) as usize;
                            
                            // Calculate which cell was clicked
                            let relative_x = mx - content_x - padding;
                            let relative_y = my - grid_y;
                            
                            if relative_x >= 0 && relative_y >= 0 {
                                let clicked_col = (relative_x / cell_w) as usize;
                                let clicked_row = (relative_y / cell_h) as usize;
                                
                                if clicked_col < cols && clicked_row < visible_rows {
                                    let clicked_display_idx = clicked_row * cols + clicked_col;
                                    let clicked_file_idx = fm.scroll_offset + clicked_display_idx;
                                    
                                    if clicked_file_idx < fm.files.len() {
                                        // Double-click detection: if same item clicked again
                                        if fm.selected == Some(clicked_file_idx) {
                                            // Double click - open the item
                                            // First check if it's a file (not directory)
                                            if let Some(file_path) = fm.get_selected_file_path() {
                                                // Open file in editor
                                                drop(gui);
                                                open_file_in_editor(&file_path);
                                                let mut gui = GUI.lock();
                                                if let Some(state) = &mut *gui {
                                                    state.needs_full_redraw = true;
                                                }
                                                return;
                                            } else if fm.open_selected() {
                                                // It was a directory - opened successfully
                                                state.needs_window_redraw = true;
                                            }
                                        } else {
                                            // Single click - select item
                                            fm.selected = Some(clicked_file_idx);
                                            state.needs_window_redraw = true;
                                        }
                                    } else {
                                        fm.selected = None;
                                        state.needs_window_redraw = true;
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Handle text editor content clicks
                if let Some(w) = state.windows.iter_mut().find(|w| w.id == id && w.focused) {
                    if let WindowContent::TextEditor(editor) = &mut w.content {
                        let content_x = w.x + 1;
                        let content_y = w.y + 32;  // After title bar
                        let content_w = (w.width as i32) - 2;
                        let toolbar_h: i32 = 36;
                        
                        // Button positions (must match rendering)
                        let btn_y = content_y + 6;
                        let btn_h = 24;
                        let btn_spacing = 8;
                        
                        // Save button
                        let save_x = content_x + 10;
                        let save_w = 56;
                        
                        // Save As button  
                        let saveas_x = save_x + save_w + btn_spacing;
                        let saveas_w = 72;
                        
                        // Undo button
                        let undo_x = saveas_x + saveas_w + btn_spacing;
                        let undo_w = 52;
                        
                        // Redo button
                        let redo_x = undo_x + undo_w + 4;
                        
                        // Check toolbar clicks
                        if my >= btn_y && my < btn_y + btn_h {
                            // Save
                            if mx >= save_x && mx < save_x + save_w {
                                editor.save_file();
                                state.needs_window_redraw = true;
                            }
                            // Save As
                            else if mx >= saveas_x && mx < saveas_x + saveas_w {
                                let (default_name, current_dir, editor_content) = {
                                    if let Some(ref path) = editor.filename {
                                        if let Some(pos) = path.rfind('/') {
                                            (String::from(&path[pos+1..]), String::from(&path[..pos]), editor.content())
                                        } else {
                                            (path.clone(), String::from("/"), editor.content())
                                        }
                                    } else {
                                        (String::from("untitled.txt"), String::from("/home/user"), editor.content())
                                    }
                                };

                                drop(gui);
                                let mut gui = GUI.lock();
                                if let Some(state) = &mut *gui {
                                    let prompt_id = state.create_window("Save As", 260, 180, 560, 360);
                                    if let Some(new_w) = state.windows.iter_mut().find(|w| w.id == prompt_id) {
                                        let sas = SaveAsState::new(&current_dir, &default_name, &editor_content);
                                        new_w.content = WindowContent::SaveAs(sas);
                                    }
                                    state.needs_full_redraw = true;
                                }
                                return;
                            }
                            // Undo
                            else if mx >= undo_x && mx < undo_x + undo_w {
                                editor.undo();
                                state.needs_window_redraw = true;
                            }
                            // Redo
                            else if mx >= redo_x && mx < redo_x + undo_w {
                                editor.redo();
                                state.needs_window_redraw = true;
                            }
                        }
                        // Click in text area - position cursor
                        else if my >= content_y + toolbar_h {
                            let gutter_width: i32 = 48;
                            let text_padding: i32 = 8;
                            let line_height: i32 = 18;
                            let char_width: i32 = 8;
                            let text_x = content_x + gutter_width + text_padding;
                            let text_y = content_y + toolbar_h + 4;
                            
                            if mx >= text_x {
                                let click_col = ((mx - text_x) / char_width) as usize + editor.scroll_x;
                                let click_row = ((my - text_y) / line_height) as usize + editor.scroll_y;
                                
                                // Set cursor position
                                editor.cursor_line = click_row.min(editor.lines.len().saturating_sub(1));
                                let line_len = editor.lines[editor.cursor_line].len();
                                editor.cursor_col = click_col.min(line_len);
                                state.needs_window_redraw = true;
                            }
                        }
                    }
                    // Handle SaveAs dialog clicks
                    if let WindowContent::SaveAs(sas) = &mut w.content {
                        let content_x: i32 = w.x + 1;
                        let content_y: i32 = w.y + 32;
                        let content_w: i32 = (w.width as i32) - 2;
                        let content_h: i32 = (w.height as i32) - 33;
                        let toolbar_h: i32 = 36;

                        // Toolbar Save/Cancel
                        if my >= content_y && my < content_y + toolbar_h {
                            let btn_x = content_x + 12;
                            let btn_y = content_y + 6;
                            let btn_w = 80;
                            let btn_h = 24;
                            // Save
                            if mx >= btn_x && mx < btn_x + btn_w && my >= btn_y && my < btn_y + btn_h {
                                // Construct path
                                let path = if sas.current_dir == "/" {
                                    alloc::format!("/{}", sas.filename)
                                } else {
                                    alloc::format!("{}/{}", sas.current_dir, sas.filename)
                                };
                                let _ = crate::fs::write_file(&path, sas.content.as_bytes());
                                // Close dialog and open saved file in editor
                                state.close_window(id);
                                drop(gui);
                                open_file_in_editor(&path);
                                let mut gui = GUI.lock();
                                if let Some(state) = &mut *gui {
                                    state.needs_full_redraw = true;
                                }
                                return;
                            }
                            // Cancel
                            let cancel_x = btn_x + btn_w + 12;
                            if mx >= cancel_x && mx < cancel_x + btn_w && my >= btn_y && my < btn_y + btn_h {
                                state.close_window(id);
                                state.needs_full_redraw = true;
                                return;
                            }
                        }

                        // Directory list area clicks
                        // Match the layout from drawing code:
                        // toolbar_h = 36, input_y = content_y + 36 + 12, box_y = input_y + 18, box_h = 28
                        // dir_label_y = box_y + box_h + 12, list_y = dir_label_y + 24, list_top = list_y + 20
                        let toolbar_h = 36i32;
                        let input_y = content_y + toolbar_h + 12;
                        let box_y = input_y + 18;
                        let box_h = 28i32;
                        let dir_label_y = box_y + box_h + 12;
                        let list_label_y = dir_label_y + 24;
                        let list_top = list_label_y + 20;
                        let list_x = content_x + 12;
                        let list_w = content_w - 24;
                        let list_h = content_h - (list_top - content_y) - 12;
                        let line_h = 24i32;
                        
                        if my >= list_top as i32 && my < (list_top + list_h) as i32 && mx >= list_x as i32 && mx < (list_x + list_w) as i32 {
                            let rel_y = my - list_top as i32 - 4;
                            if rel_y >= 0 {
                                let row = (rel_y / line_h) as usize;
                                if sas.current_dir != "/" {
                                    // Parent directory is first row
                                    if row == 0 {
                                        // Go up to parent directory
                                        if let Some(pos) = sas.current_dir.rfind('/') {
                                            if pos == 0 {
                                                sas.current_dir = String::from("/");
                                            } else {
                                                sas.current_dir = String::from(&sas.current_dir[..pos]);
                                            }
                                        }
                                        sas.refresh();
                                        state.needs_window_redraw = true;
                                        return;
                                    } else {
                                        // Directory entries (offset by 1 for parent dir row)
                                        let idx = sas.scroll_offset + (row - 1);
                                        if idx < sas.dirs.len() {
                                            let dir = &sas.dirs[idx];
                                            if sas.current_dir == "/" {
                                                sas.current_dir = alloc::format!("/{}", dir.name);
                                            } else {
                                                sas.current_dir = alloc::format!("{}/{}", sas.current_dir, dir.name);
                                            }
                                            sas.refresh();
                                            state.needs_window_redraw = true;
                                            return;
                                        }
                                    }
                                } else {
                                    // At root - no parent directory row
                                    let idx = sas.scroll_offset + row;
                                    if idx < sas.dirs.len() {
                                        let dir = &sas.dirs[idx];
                                        sas.current_dir = alloc::format!("/{}", dir.name);
                                        sas.refresh();
                                        state.needs_window_redraw = true;
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Check desktop icons
            if !handled {
                // Check dock clicks
                let dock_item_size: i32 = 48;
                let dock_padding: i32 = 8;
                let dock_spacing: i32 = 4;
                let num_items = state.dock_items.len() as i32;
                let bb_width = {
                    let fb = FRAMEBUFFER.lock();
                    fb.width as i32
                };
                let bb_height = {
                    let fb = FRAMEBUFFER.lock();
                    fb.height as i32
                };
                
                let dock_width = num_items * dock_item_size + (num_items + 1) * dock_spacing + dock_padding * 2;
                let dock_height = dock_item_size + dock_padding * 2;
                let dock_x = (bb_width - dock_width) / 2;
                let dock_y = bb_height - dock_height - 8;
                
                let mut action: Option<IconAction> = None;
                
                if my >= dock_y && my < dock_y + dock_height && mx >= dock_x && mx < dock_x + dock_width {
                    for (i, item) in state.dock_items.iter().enumerate() {
                        let item_x = dock_x + dock_padding + dock_spacing + (i as i32 * (dock_item_size + dock_spacing));
                        let item_y = dock_y + dock_padding;
                        
                        if mx >= item_x && mx < item_x + dock_item_size &&
                           my >= item_y && my < item_y + dock_item_size {
                            action = Some(match item.action {
                                IconAction::OpenTerminal => IconAction::OpenTerminal,
                                IconAction::OpenAbout => IconAction::OpenAbout,
                                IconAction::OpenFiles => IconAction::OpenFiles,
                                IconAction::OpenEditor => IconAction::OpenEditor,
                            });
                            break;
                        }
                    }
                }
                
                if let Some(act) = action {
                    match act {
                        IconAction::OpenTerminal => {
                            let id = state.create_window("Terminal", 200, 80, 600, 400);
                            if let Some(w) = state.windows.iter_mut().find(|w| w.id == id) {
                                w.content = WindowContent::Terminal(TerminalState {
                                    buffer: String::new(),
                                    input: String::new(),
                                    cursor_visible: true,
                                    scroll_offset: 0,
                                });
                            }
                            state.needs_full_redraw = true;
                        }
                        IconAction::OpenAbout => {
                            let id = state.create_window("System Info", 250, 80, 360, 480);
                            if let Some(w) = state.windows.iter_mut().find(|w| w.id == id) {
                                w.content = WindowContent::About(AboutState::new());
                            }
                            state.needs_full_redraw = true;
                        }
                        IconAction::OpenFiles => {
                            let id = state.create_window("Files", 250, 100, 550, 450);
                            if let Some(w) = state.windows.iter_mut().find(|w| w.id == id) {
                                w.content = WindowContent::FileManager(FileManagerState::new("/"));
                            }
                            state.needs_window_redraw = true;
                        }
                        IconAction::OpenEditor => {
                            let id = state.create_window("Text Editor", 150, 50, 700, 500);
                            if let Some(w) = state.windows.iter_mut().find(|w| w.id == id) {
                                w.content = WindowContent::TextEditor(TextEditorState::new());
                            }
                            state.needs_full_redraw = true;
                        }
                    }
                }
            }
        }
        
        state.mouse_prev_left = left;
        state.mouse_prev_right = right;
        state.mouse_x = mx;
        state.mouse_y = my;
    }
}

/// Handle keyboard input for GUI (special keys)
pub fn handle_key_event(event: &crate::drivers::keyboard::KeyEvent) {
    use crate::drivers::keyboard::KeyCode;
    
    if !event.pressed {
        return;
    }
    
    let mut gui = GUI.lock();
    if let Some(state) = &mut *gui {
        // Find focused window
        for window in state.windows.iter_mut().rev() {
            if window.focused {
                match &mut window.content {
                    WindowContent::Terminal(term) => {
                        match event.keycode {
                            KeyCode::Up => {
                                // Scroll up in terminal
                                term.scroll_offset = term.scroll_offset.saturating_add(1);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::Down => {
                                // Scroll down in terminal
                                term.scroll_offset = term.scroll_offset.saturating_sub(1);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::PageUp => {
                                term.scroll_offset = term.scroll_offset.saturating_add(10);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::PageDown => {
                                term.scroll_offset = term.scroll_offset.saturating_sub(10);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::Home => {
                                // Go to beginning of input
                                state.needs_window_redraw = true;
                            }
                            KeyCode::End => {
                                // Go to end of input, reset scroll
                                term.scroll_offset = 0;
                                state.needs_window_redraw = true;
                            }
                            KeyCode::Delete => {
                                // Delete is like backspace in simple terminal
                                term.input.pop();
                                term.scroll_offset = 0;
                                state.needs_window_redraw = true;
                            }
                            _ => {}
                        }
                    }
                    WindowContent::FileManager(fm) => {
                        let cols = 8usize; // Approximate columns in grid
                        match event.keycode {
                            KeyCode::Up => {
                                // Move selection up one row
                                if let Some(sel) = fm.selected {
                                    if sel >= cols {
                                        fm.selected = Some(sel - cols);
                                        state.needs_window_redraw = true;
                                    }
                                } else if !fm.files.is_empty() {
                                    fm.selected = Some(0);
                                    state.needs_window_redraw = true;
                                }
                            }
                            KeyCode::Down => {
                                // Move selection down one row
                                if let Some(sel) = fm.selected {
                                    if sel + cols < fm.files.len() {
                                        fm.selected = Some(sel + cols);
                                        state.needs_window_redraw = true;
                                    }
                                } else if !fm.files.is_empty() {
                                    fm.selected = Some(0);
                                    state.needs_window_redraw = true;
                                }
                            }
                            KeyCode::Left => {
                                // Move selection left
                                if let Some(sel) = fm.selected {
                                    if sel > 0 {
                                        fm.selected = Some(sel - 1);
                                        state.needs_window_redraw = true;
                                    }
                                } else if !fm.files.is_empty() {
                                    fm.selected = Some(0);
                                    state.needs_window_redraw = true;
                                }
                            }
                            KeyCode::Right => {
                                // Move selection right
                                if let Some(sel) = fm.selected {
                                    if sel + 1 < fm.files.len() {
                                        fm.selected = Some(sel + 1);
                                        state.needs_window_redraw = true;
                                    }
                                } else if !fm.files.is_empty() {
                                    fm.selected = Some(0);
                                    state.needs_window_redraw = true;
                                }
                            }
                            KeyCode::Enter => {
                                // Open selected item
                                if fm.open_selected() {
                                    state.needs_window_redraw = true;
                                }
                            }
                            KeyCode::Backspace => {
                                // Go back (like pressing back button)
                                if fm.go_back() {
                                    state.needs_window_redraw = true;
                                }
                            }
                            KeyCode::PageUp => {
                                // Scroll up
                                fm.scroll_offset = fm.scroll_offset.saturating_sub(8);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::PageDown => {
                                // Scroll down
                                let max_scroll = fm.files.len().saturating_sub(16);
                                fm.scroll_offset = (fm.scroll_offset + 8).min(max_scroll);
                                state.needs_window_redraw = true;
                            }
                            _ => {}
                        }
                    }
                    WindowContent::About(about_state) => {
                        let max_scroll: i32 = 150; // Content exceeds visible by ~150px
                        match event.keycode {
                            KeyCode::Up => {
                                about_state.scroll_offset = (about_state.scroll_offset - 20).max(0);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::Down => {
                                about_state.scroll_offset = (about_state.scroll_offset + 20).min(max_scroll);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::PageUp => {
                                about_state.scroll_offset = (about_state.scroll_offset - 80).max(0);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::PageDown => {
                                about_state.scroll_offset = (about_state.scroll_offset + 80).min(max_scroll);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::Home => {
                                about_state.scroll_offset = 0;
                                state.needs_window_redraw = true;
                            }
                            KeyCode::End => {
                                about_state.scroll_offset = max_scroll;
                                state.needs_window_redraw = true;
                            }
                            _ => {}
                        }
                    }
                    WindowContent::TextEditor(editor) => {
                        // Handle special keys for text editor
                        match event.keycode {
                            KeyCode::Up => {
                                editor.move_up();
                                editor.ensure_cursor_visible(25, 80);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::Down => {
                                editor.move_down();
                                editor.ensure_cursor_visible(25, 80);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::Left => {
                                editor.move_left();
                                editor.ensure_cursor_visible(25, 80);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::Right => {
                                editor.move_right();
                                editor.ensure_cursor_visible(25, 80);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::Home => {
                                editor.move_home();
                                editor.ensure_cursor_visible(25, 80);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::End => {
                                editor.move_end();
                                editor.ensure_cursor_visible(25, 80);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::PageUp => {
                                editor.page_up(20);
                                editor.ensure_cursor_visible(25, 80);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::PageDown => {
                                editor.page_down(20);
                                editor.ensure_cursor_visible(25, 80);
                                state.needs_window_redraw = true;
                            }
                            KeyCode::Delete => {
                                editor.delete_forward();
                                state.needs_window_redraw = true;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
                break;
            }
        }
    }
}

/// Handle keyboard input for GUI (printable characters)
pub fn handle_keyboard(c: char) {
    let mut gui = GUI.lock();
    if let Some(state) = &mut *gui {
        // Find focused window
        for window in state.windows.iter_mut().rev() {
            if window.focused {
                match &mut window.content {
                    WindowContent::Terminal(term) => {
                        match c {
                            '\n' | '\r' => {
                                // Reset scroll to bottom when executing command
                                term.scroll_offset = 0;
                                
                                // Execute command using shell
                                let cmd = term.input.clone();
                                term.buffer.push_str(&alloc::format!("{}> {}\n", crate::shell::get_cwd(), cmd));
                                
                                // Use the real shell command executor
                                let output = crate::shell::execute_command(&cmd);
                                
                                // Handle clear command
                                if output == "\x1b[CLEAR]" {
                                    term.buffer.clear();
                                } else if !output.is_empty() {
                                    term.buffer.push_str(&output);
                                    if !output.ends_with('\n') {
                                        term.buffer.push('\n');
                                    }
                                }
                                
                                term.input.clear();
                            }
                            '\x08' | '\x7f' => {
                                term.input.pop();
                                term.scroll_offset = 0; // Reset scroll when typing
                            }
                            '\t' => {
                                // Tab - insert spaces or handle tab completion
                                term.input.push_str("    ");
                                term.scroll_offset = 0;
                            }
                            '\x1b' => {
                                // Escape - clear current input
                                term.input.clear();
                                term.scroll_offset = 0;
                            }
                            c if c >= ' ' && c <= '~' => {
                                term.input.push(c);
                                term.scroll_offset = 0; // Reset scroll when typing
                            }
                            _ => {}
                        }
                        state.needs_window_redraw = true;
                        break;
                    }
                    WindowContent::TextEditor(editor) => {
                        match c {
                            '\n' | '\r' => {
                                editor.insert_char('\n');
                                editor.ensure_cursor_visible(25, 80);
                            }
                            '\x08' | '\x7f' => {
                                editor.delete_char();
                                editor.ensure_cursor_visible(25, 80);
                            }
                            '\t' => {
                                // Tab - insert 4 spaces
                                for _ in 0..4 {
                                    editor.insert_char(' ');
                                }
                                editor.ensure_cursor_visible(25, 80);
                            }
                            c if c >= ' ' && c <= '~' => {
                                editor.insert_char(c);
                                editor.ensure_cursor_visible(25, 80);
                            }
                            _ => {}
                        }
                        // Update cursor blink to make it visible during typing
                        editor.cursor_visible = true;
                        editor.blink_counter = 0;
                        state.needs_window_redraw = true;
                        break;
                    }
                    WindowContent::SaveAs(sas) => {
                        let save_window_id = window.id;
                        match c {
                            '\n' | '\r' => {
                                // perform save
                                let path = if sas.current_dir == "/" {
                                    alloc::format!("/{}", sas.filename)
                                } else {
                                    alloc::format!("{}/{}", sas.current_dir, sas.filename)
                                };
                                let _ = crate::fs::write_file(&path, sas.content.as_bytes());
                                // close dialog and open saved file in editor
                                drop(gui);
                                open_file_in_editor(&path);
                                let mut gui = GUI.lock();
                                if let Some(state) = &mut *gui {
                                    state.close_window(save_window_id);
                                    state.needs_full_redraw = true;
                                }
                                break;
                            }
                            '\x08' | '\x7f' => {
                                sas.filename.pop();
                            }
                            c if c >= ' ' && c <= '~' => {
                                sas.filename.push(c);
                            }
                            '\x1b' => {
                                // Escape => cancel: close dialog
                                let save_window_id = window.id;
                                drop(gui);
                                let mut gui = GUI.lock();
                                if let Some(state) = &mut *gui {
                                    state.close_window(save_window_id);
                                    state.needs_full_redraw = true;
                                }
                                break;
                            }
                            _ => {}
                        }
                        state.needs_window_redraw = true;
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Open a file in the text editor
fn open_file_in_editor(path: &str) {
    let mut gui = GUI.lock();
    if let Some(state) = &mut *gui {
        // Create editor window with the file loaded
        let title = if path.len() > 40 {
            alloc::format!("Editor - ...{}", &path[path.len()-35..])
        } else {
            alloc::format!("Editor - {}", path)
        };
        let id = state.create_window(&title, 150, 50, 700, 500);
        if let Some(w) = state.windows.iter_mut().find(|w| w.id == id) {
            let mut editor = TextEditorState::new();
            editor.load_file(path);
            w.content = WindowContent::TextEditor(editor);
        }
        state.needs_full_redraw = true;
    }
}

/// Run GUI main loop with double buffering
pub fn run() {
    kprintln!("[GUI] Starting GUI with double buffering...");
    
    loop {
        // Handle mouse input first (this updates internal state)
        handle_mouse();
        
        // Get current mouse position
        let (mx, my) = mouse::get_position();
        
        // Check keyboard
        if crate::drivers::keyboard::has_key() {
            if let Some(event) = crate::drivers::keyboard::read_key() {
                // First handle special keys (arrows, page up/down, etc.)
                handle_key_event(&event);
                
                // Then try to get printable character
                if let Some(c) = crate::drivers::keyboard::keyevent_to_char(&event) {
                    handle_keyboard(c);
                }
            }
        }
        
        // Update cursor blink for text editors
        {
            let mut gui = GUI.lock();
            if let Some(state) = &mut *gui {
                for window in &mut state.windows {
                    if let WindowContent::TextEditor(editor) = &mut window.content {
                        editor.update_blink();
                    }
                }
            }
        }
        
        // Clear needs_redraw flags after handling input
        {
            let mut gui = GUI.lock();
            if let Some(state) = &mut *gui {
                state.needs_full_redraw = false;
                state.needs_window_redraw = false;
            }
        }
        
        // Draw EVERYTHING to back buffer (no flicker because it's in memory)
        let bb = BackBuffer::new();
        draw_background(&bb);
        draw_dock(&bb);
        draw_windows(&bb);
        draw_cursor_to_bb(&bb, mx, my);
        
        // Swap back buffer to screen in one atomic operation
        swap_buffers();
        
        // Small delay
        for _ in 0..3000 {
            core::hint::spin_loop();
        }
        
        // Check exit
        let should_exit = {
            let gui = GUI.lock();
            if let Some(state) = &*gui {
                !state.running
            } else {
                true
            }
        };
        
        if should_exit {
            break;
        }
    }
}
