use anyhow;
use fehler::throws;
use path_clean::{clean, PathClean};
use pathdiff::diff_paths;
use regex::{Captures, Regex};
use std::path::{Path, PathBuf};

// Currently not support:
// - Interwiki links
// - Markdown reference-style links
// diary:file:local:

lazy_static! {
    static ref DEFAULT_LINK_RE: Regex = Regex::new(
        r"(?x)
        (?P<left>\[\[\s*)
        ((?P<prefix>diary|file|local):)?(?P<path>(?-x:[^#|]+?))
        (?P<right>(?-x:#.*)?(\|.*)*\]\])
    "
    )
    .unwrap();
    static ref WIKI_INCLUDE_RE: Regex = Regex::new(
        r"(?x)
        (?P<left>\{\{\s*)
        ((?P<prefix>diary|file|local):)?(?P<path>(?-x:[^#|]+?))
        (?P<right>(?-x:#.*)?(\|.*)*\}\})
    "
    )
    .unwrap();
    static ref MD_LINK_RE: Regex = Regex::new(
        r"(?x)
        (?P<left>\[.*\]\()
        ((?P<prefix>diary|file|local):)?(?P<path>(?-x:[^#|]+?))
        (?P<right>(?-x:#.*)?\))
    "
    )
    .unwrap();
}

#[allow(dead_code)]
type Error = anyhow::Error;

struct AbsolutePath {
    path: PathBuf,
}

impl AbsolutePath {
    fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf().clean(),
        }
    }

    fn is_in_diary(&self) -> bool {
        self.path
            .to_str()
            .map(|s| s.contains("/diary/"))
            .unwrap_or(false)
    }

    fn get_path(&self) -> &Path {
        &self.path
    }

    fn get_file_name(&self) -> Option<String> {
        self.path
            .with_extension("")
            .file_name()
            .and_then(|x| x.to_str().map(String::from))
    }
}

impl PartialEq for AbsolutePath {
    fn eq(&self, other: &Self) -> bool {
        match (self.path.extension(), other.path.extension()) {
            (Some(_), Some(_)) => self.path == other.path,
            _ => self.path.with_extension("") == other.path.with_extension(""),
        }
    }
}

struct Link<'a> {
    prefix: Option<&'a str>,
    path: &'a str,
}

impl<'a> Link<'a> {
    fn new(prefix: Option<&'a str>, path: &'a str) -> Self {
        Self {
            prefix,
            path: path.trim(),
        }
    }

    fn display(&self) -> String {
        format!("{}:{}", self.prefix.unwrap_or(""), self.path)
    }
}

struct Wiki {
    wiki_root: PathBuf,
    content_path: PathBuf,
}

impl<'a> Wiki {
    fn new(wiki_root: PathBuf, content_path: PathBuf) -> Self {
        Wiki {
            wiki_root,
            content_path,
        }
    }

    fn get_absolute_path(&self, link: &Link) -> AbsolutePath {
        let mut root;
        match link.prefix {
            Some("diary") => {
                root = self.wiki_root.to_path_buf();
                root.push("diary");
            }
            _ => {
                root = if link.path.starts_with('/') {
                    self.wiki_root.to_path_buf()
                } else {
                    self.content_path
                        .parent()
                        .expect("get_absolute_path: Wiki file should have a parent")
                        .to_path_buf()
                }
            }
        };
        AbsolutePath::new(root.join(link.path.trim_start_matches('/')))
    }

    fn get_relative_path(&self, to: &AbsolutePath) -> Option<String> {
        // Strip file extension
        diff_paths(
            to.get_path().with_extension(""),
            &self
                .content_path
                .parent()
                .expect("get_relative_path: Wiki file should have a parent"),
        )
        .and_then(|p| p.to_str().map(String::from))
    }

    fn replace_links(&self, content: &str, from: AbsolutePath, to: AbsolutePath) -> String {
        let replace = |caps: &Captures| {
            let origin = caps[0].to_owned();
            let prefix = caps.name("prefix").map(|m| m.as_str());
            let path = &caps.name("path").expect("Should captured with name link");
            let link = Link::new(prefix, path.as_str());

            if self.get_absolute_path(&link) != from {
                return origin;
            }

            let left = &caps
                .name("left")
                .expect("Should captured with left side of link")
                .as_str();
            let right = &caps
                .name("right")
                .expect("Should captured with right side of link")
                .as_str();

            let replaced = if to.is_in_diary() {
                to.get_file_name()
                    .map(|file_name| format!("diary:{}", file_name))
                    .unwrap_or_else(|| link.display())
            } else {
                self.get_relative_path(&to)
                    .map(|relative_path| {
                        prefix
                            .filter(|s| *s != "diary")
                            .map(|s| {
                                let mut s = s.to_owned();
                                s.push(':');
                                s.push_str(&relative_path);
                                s
                            })
                            .unwrap_or_else(|| relative_path.to_owned())
                    })
                    .unwrap_or_else(|| link.display())
            };
            format!(
                "{}{}{}",
                left,
                replaced,
                if right.starts_with('|') {
                    format!(" {}", right)
                } else {
                    (*right).to_string()
                }
            )
        };
        let content = MD_LINK_RE.replace_all(content, replace);
        let content = DEFAULT_LINK_RE.replace_all(&content, replace);
        WIKI_INCLUDE_RE.replace_all(&content, replace).into_owned()
    }
}

pub fn rename(wiki_root: PathBuf, old_path: PathBuf, new_name: &str) {
    unimplemented!()
}

#[cfg(test)]
mod test_links_regex {
    use super::*;

    #[test]
    fn it_capture_plain_link() {
        let cap = DEFAULT_LINK_RE.captures("[[This is a link]]").unwrap();
        assert_eq!(&cap[0], "[[This is a link]]");
        assert_eq!(&cap["path"], "This is a link");
    }
    #[test]
    fn it_capture_plain_link_with_prefix() {
        let cap = DEFAULT_LINK_RE
            .captures("[[diary:This is a link]]")
            .unwrap();
        assert_eq!(&cap["path"], "This is a link");
        assert_eq!(&cap["prefix"], "diary");

        let cap = DEFAULT_LINK_RE.captures("[[file:This is a link]]").unwrap();
        assert_eq!(&cap["path"], "This is a link");
        assert_eq!(&cap["prefix"], "file");

        let cap = DEFAULT_LINK_RE
            .captures("[[local:This is a link]]")
            .unwrap();
        assert_eq!(&cap["path"], "This is a link");
        assert_eq!(&cap["prefix"], "local");
    }
    #[test]
    fn it_capture_plain_link_with_description() {
        let cap = DEFAULT_LINK_RE
            .captures("[[This is a link|Description of the link]]")
            .unwrap();
        assert_eq!(&cap["path"], "This is a link");
    }

    #[test]
    fn it_capture_transclusion_link() {
        let cap = WIKI_INCLUDE_RE
            .captures("{{file:../../images/vimwiki_logo.png}}")
            .unwrap();
        assert_eq!(&cap["path"], "../../images/vimwiki_logo.png");
        assert_eq!(&cap["prefix"], "file");
    }
    #[test]
    fn it_capture_transclusion_link_complex() {
        let cap = WIKI_INCLUDE_RE
            .captures(
                r#"{{http://.../vimwiki_logo.png|cool stuff|style="width:150px;height:120px;"}}"#,
            )
            .unwrap();
        assert_eq!(&cap["path"], "http://.../vimwiki_logo.png");
    }
    #[test]
    fn it_capture_markdown_link() {
        let cap = MD_LINK_RE.captures("[Looks like this](URL)").unwrap();
        assert_eq!(&cap["path"], "URL");
        assert!(&cap.name("prefix").is_none());

        let cap = MD_LINK_RE.captures("[Looks like this](diary:URL)").unwrap();
        assert_eq!(&cap["path"], "URL");
        assert_eq!(&cap["prefix"], "diary");

        let cap = MD_LINK_RE.captures("[Looks like this](file:URL)").unwrap();
        assert_eq!(&cap["path"], "URL");
        assert_eq!(&cap["prefix"], "file");

        let cap = MD_LINK_RE.captures("[Looks like this](local:URL)").unwrap();
        assert_eq!(&cap["path"], "URL");
        assert_eq!(&cap["prefix"], "local");
    }
    #[test]
    fn it_capture_wiki_link_with_anchors() {
        let cap = DEFAULT_LINK_RE
            .captures("[[This is a link#Tommorrow]]")
            .unwrap();
        assert_eq!(&cap["path"], "This is a link");
    }
    #[test]
    fn it_capture_markdown_link_with_anchors() {
        let cap = MD_LINK_RE
            .captures("[Looks like this](URL#anchor)")
            .unwrap();
        assert_eq!(&cap["path"], "URL");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow;
    use fehler::throws;
    use std::path::{Path, PathBuf};

    type Error = anyhow::Error;

    #[test]
    fn it_replace_diary_links() {
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/books/note.md");
        let wiki = Wiki::new(wiki_root, content_path);
        let content = r#"
        Here is a [diary](diary:2010-01-01).
        "#;
        assert_eq!(
            wiki.replace_links(
                content,
                AbsolutePath::new("/dropbox/vimwiki/diary/2010-01-01.md"),
                AbsolutePath::new("/dropbox/vimwiki/diary/2020-02-02.md")
            ),
            r#"
        Here is a [diary](diary:2020-02-02).
        "#
        );
    }

    #[test]
    fn it_replace_diary_links_to_non_dairy() {
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/books/note.md");
        let wiki = Wiki::new(wiki_root, content_path);
        let content = r#"
        Here is a [diary](diary:2010-01-01).
        "#;
        assert_eq!(
            wiki.replace_links(
                content,
                AbsolutePath::new("/dropbox/vimwiki/diary/2010-01-01.md"),
                AbsolutePath::new("/dropbox/vimwiki/non-dairy.md")
            ),
            r#"
        Here is a [diary](../non-dairy).
        "#
        );
    }

    #[test]
    fn it_replace_absolute_link() {
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/books/note.md");
        let wiki = Wiki::new(wiki_root, content_path);
        let content = r#"
        Here is a [absolute to root](/link).
        "#;
        assert_eq!(
            wiki.replace_links(
                content,
                AbsolutePath::new("/dropbox/vimwiki/link.md"),
                AbsolutePath::new("/dropbox/vimwiki/renamed.md")
            ),
            r#"
        Here is a [absolute to root](../renamed).
        "#
        );
    }

    #[test]
    fn it_replace_all_matched_links() {
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/books/note.md");
        let wiki = Wiki::new(wiki_root, content_path);
        let content = r#"
        - [local link relative link](local:./link).
        - [file link](file:link).
        - [reserve link](other)
        - [[link | default wiki link]]
        - {{ link | transclusion link }}
        "#;
        assert_eq!(
            wiki.replace_links(
                content,
                AbsolutePath::new("/dropbox/vimwiki/books/link.md"),
                AbsolutePath::new("/dropbox/vimwiki/books/renamed.md")
            ),
            r#"
        - [local link relative link](local:renamed).
        - [file link](file:renamed).
        - [reserve link](other)
        - [[renamed | default wiki link]]
        - {{ renamed | transclusion link }}
        "#
        );
    }
}
