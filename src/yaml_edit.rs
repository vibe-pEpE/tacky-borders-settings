//! Minimal, comment-preserving editing of a YAML file.
//!
//! We never re-serialize the whole config. Instead we locate the lines that belong to one key
//! (e.g. `global.border_width`) and replace only those lines. Everything else in the file —
//! comments, window rules, settings this app doesn't know about — stays exactly as it was.

/// New value for a key.
pub enum NewValue {
    /// Written on the same line as the key: `key: value`
    Inline(String),
    /// Written as child lines below `key:` (lines are relative; indentation is added for you)
    Block(Vec<String>),
}

pub struct YamlDoc {
    lines: Vec<String>,
    eol: &'static str,
    trailing_newline: bool,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with('#')
}

fn is_seq_item(line: &str) -> bool {
    let t = line.trim_start();
    t == "-" || t.starts_with("- ")
}

/// Returns the mapping key on this line, if the line is `key:` or `key: value`.
fn key_of(line: &str) -> Option<&str> {
    let t = line.trim_start();
    if t.starts_with('#') || t.starts_with('-') {
        return None;
    }
    let colon = t.find(':')?;
    let key = &t[..colon];
    let after = &t[colon + 1..];
    let valid_key = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    let valid_after = after.is_empty() || after.starts_with(' ') || after.starts_with('\t');
    (valid_key && valid_after).then_some(key)
}

/// True if the key line has nothing after the colon (except maybe a comment).
fn has_no_inline_value(line: &str) -> bool {
    let t = line.trim_start();
    match t.find(':') {
        Some(c) => {
            let rest = t[c + 1..].trim();
            rest.is_empty() || rest.starts_with('#')
        }
        None => false,
    }
}

impl YamlDoc {
    pub fn parse(text: &str) -> Self {
        let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
        Self {
            lines: text.lines().map(|l| l.to_string()).collect(),
            eol,
            trailing_newline: text.ends_with('\n') || text.is_empty(),
        }
    }

    pub fn to_text(&self) -> String {
        let mut s = self.lines.join(self.eol);
        if self.trailing_newline {
            s.push_str(self.eol);
        }
        s
    }

    /// Index one past the last line belonging to the key on `key_line`.
    fn block_end(&self, key_line: usize) -> usize {
        let key_indent = indent_of(&self.lines[key_line]);
        let allow_seq = has_no_inline_value(&self.lines[key_line]);
        let mut last = key_line;
        let mut i = key_line + 1;
        while i < self.lines.len() {
            let line = &self.lines[i];
            if is_blank(line) {
                i += 1;
                continue;
            }
            let ind = indent_of(line);
            let belongs = ind > key_indent || (allow_seq && ind == key_indent && is_seq_item(line));
            if !belongs {
                break;
            }
            last = i;
            i += 1;
        }
        last + 1
    }

    /// Indentation used by the direct children in [start, end), if there are any.
    fn child_indent(&self, start: usize, end: usize) -> Option<usize> {
        self.lines[start..end]
            .iter()
            .find(|l| !is_blank(l) && !is_comment(l))
            .map(|l| indent_of(l))
    }

    fn find_key(&self, start: usize, end: usize, indent: usize, key: &str) -> Option<usize> {
        (start..end).find(|&i| {
            let l = &self.lines[i];
            !is_comment(l) && indent_of(l) == indent && key_of(l) == Some(key)
        })
    }

    /// Last non-blank line index in [start, end), or None.
    fn last_content(&self, start: usize, end: usize) -> Option<usize> {
        (start..end).rev().find(|&i| !is_blank(&self.lines[i]))
    }

    fn render(indent: usize, key: &str, value: &NewValue) -> Vec<String> {
        let pad = " ".repeat(indent);
        match value {
            NewValue::Inline(v) => vec![format!("{pad}{key}: {v}")],
            NewValue::Block(children) => {
                let mut out = vec![format!("{pad}{key}:")];
                let child_pad = " ".repeat(indent + 2);
                out.extend(children.iter().map(|c| format!("{child_pad}{c}")));
                out
            }
        }
    }

    /// Set `path` (e.g. `["global", "border_width"]`) to `value`, creating parents if needed.
    pub fn set(&mut self, path: &[&str], value: NewValue) {
        assert!(!path.is_empty());
        let mut start = 0;
        let mut end = self.lines.len();
        let mut parent_indent: Option<usize> = None;

        for (depth, key) in path.iter().enumerate() {
            let default_indent = parent_indent.map_or(0, |p| p + 2);
            let indent = match parent_indent {
                None => 0,
                Some(_) => self.child_indent(start, end).unwrap_or(default_indent),
            };
            let is_last = depth == path.len() - 1;

            match self.find_key(start, end, indent, key) {
                Some(line) => {
                    let block_end = self.block_end(line);
                    if is_last {
                        let new_lines = Self::render(indent, key, &value);
                        self.lines.splice(line..block_end, new_lines);
                        return;
                    }
                    // A parent key written inline (e.g. `animations: {}`) can't be extended
                    // line-by-line, so turn it into an empty block first.
                    if !has_no_inline_value(&self.lines[line]) {
                        self.lines.splice(
                            line..block_end,
                            [format!("{}{}:", " ".repeat(indent), key)],
                        );
                        start = line + 1;
                        end = line + 1;
                    } else {
                        start = line + 1;
                        end = block_end;
                    }
                    parent_indent = Some(indent);
                }
                None => {
                    // Build the rest of the path as nested lines and insert after the region.
                    let mut new_lines = Vec::new();
                    let mut ind = indent;
                    for k in &path[depth..path.len() - 1] {
                        new_lines.push(format!("{}{}:", " ".repeat(ind), k));
                        ind += 2;
                    }
                    new_lines.extend(Self::render(ind, path[path.len() - 1], &value));
                    let insert_at = match parent_indent {
                        None => self.lines.len(),
                        Some(_) => self.last_content(start, end).map_or(start, |i| i + 1),
                    };
                    self.lines.splice(insert_at..insert_at, new_lines);
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# top comment
watch_config_changes: True

global:
  # border_width: Width of the border
  border_width: 3

  # border_offset: comment
  border_offset: -1

  active_color:
    colors: [\"#6274e7\", \"#8752a3\"]
    direction: 45deg

  inactive_color:
    colors: [\"#30304f\", \"#363c69\"]
    direction:
      start: [0.0, 1.0]
      end: [1.0, 0.0]

  animations:
    active:
      - type: Fade
        duration: 200
    fps: 60
    enabled: False

window_rules:
  - match: Process
    name: \"zebar\"
    enabled: False
";

    fn edit(path: &[&str], v: NewValue) -> String {
        let mut d = YamlDoc::parse(SAMPLE);
        d.set(path, v);
        d.to_text()
    }

    #[test]
    fn untouched_roundtrip() {
        assert_eq!(YamlDoc::parse(SAMPLE).to_text(), SAMPLE);
    }

    #[test]
    fn replace_scalar_keeps_comments() {
        let out = edit(&["global", "border_width"], NewValue::Inline("5".into()));
        assert_eq!(out, SAMPLE.replace("  border_width: 3", "  border_width: 5"));
    }

    #[test]
    fn replace_block_with_scalar() {
        let out = edit(&["global", "inactive_color"], NewValue::Inline("\"#ff0000\"".into()));
        let expected = SAMPLE.replace(
            "  inactive_color:\n    colors: [\"#30304f\", \"#363c69\"]\n    direction:\n      start: [0.0, 1.0]\n      end: [1.0, 0.0]\n",
            "  inactive_color: \"#ff0000\"\n",
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn replace_scalar_with_block() {
        let out = edit(
            &["global", "border_offset"],
            NewValue::Block(vec!["top: 1".into(), "left: 2".into()]),
        );
        assert!(out.contains("  border_offset:\n    top: 1\n    left: 2\n\n  active_color:"));
        assert!(out.contains("  # border_offset: comment\n"));
    }

    #[test]
    fn nested_existing_key() {
        let out = edit(&["global", "animations", "enabled"], NewValue::Inline("True".into()));
        assert_eq!(out, SAMPLE.replace("    enabled: False\n\nwindow", "    enabled: True\n\nwindow"));
        // window rule untouched
        assert!(out.contains("    name: \"zebar\"\n    enabled: False\n"));
    }

    #[test]
    fn insert_missing_nested_key() {
        let out = edit(&["global", "effects", "enabled"], NewValue::Inline("True".into()));
        assert!(out.contains("    enabled: False\n  effects:\n    enabled: True\n\nwindow_rules:"));
    }

    #[test]
    fn insert_missing_top_level() {
        let mut d = YamlDoc::parse("watch_config_changes: True\n");
        d.set(&["global", "border_width"], NewValue::Inline("4".into()));
        assert_eq!(d.to_text(), "watch_config_changes: True\nglobal:\n  border_width: 4\n");
    }

    #[test]
    fn custom_indentation_respected() {
        let mut d = YamlDoc::parse("global:\n    border_width: 3\n    border_radius: Auto\n");
        d.set(&["global", "border_radius"], NewValue::Inline("Round".into()));
        assert_eq!(d.to_text(), "global:\n    border_width: 3\n    border_radius: Round\n");
    }

    #[test]
    fn crlf_preserved() {
        let mut d = YamlDoc::parse("global:\r\n  border_width: 3\r\n");
        d.set(&["global", "border_width"], NewValue::Inline("7".into()));
        assert_eq!(d.to_text(), "global:\r\n  border_width: 7\r\n");
    }

    #[test]
    fn does_not_match_commented_key() {
        let mut d = YamlDoc::parse("global:\n  # border_width: 9\n  border_width: 3\n");
        d.set(&["global", "border_width"], NewValue::Inline("1".into()));
        assert_eq!(d.to_text(), "global:\n  # border_width: 9\n  border_width: 1\n");
    }
}
