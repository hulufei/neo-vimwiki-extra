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
        (?P<left>\[\[)
        ((?P<prefix>diary|file|local):)?(?P<path>(?-x:[^#|]+))
        (?P<right>(?-x:#.*)?(\|.*)*\]\])
    "
    )
    .unwrap();
    static ref WIKI_INCLUDE_RE: Regex = Regex::new(
        r"(?x)
        (?P<left>\{\{)
        ((?P<prefix>diary|file|local):)?(?P<path>(?-x:[^#|]+))
        (?P<right>(?-x:#.*)?(\|.*)*\}\})
    "
    )
    .unwrap();
    static ref MD_LINK_RE: Regex = Regex::new(
        r"(?x)
        (?P<left>\[.*\]\()
        ((?P<prefix>diary|file|local):)?(?P<path>(?-x:[^#|]+))
        (?P<right>(?-x:#.*)?\))
    "
    )
    .unwrap();
}

#[allow(dead_code)]
type Error = anyhow::Error;

struct Link<'a> {
    prefix: Option<&'a str>,
    path: &'a str,
}

impl<'a> Link<'a> {
    fn new(prefix: Option<&'a str>, path: &'a str) -> Self {
        Self { prefix, path }
    }

    fn get_full_path(&self, wiki_root: &Path, content_path: &Path) -> PathBuf {
        let mut root;
        match self.prefix {
            Some("diary") => {
                root = wiki_root.to_path_buf();
                root.push("diary");
            }
            _ => {
                root = content_path
                    .parent()
                    .expect("Wiki file should have a parent")
                    .to_path_buf();
            }
        };
        root.join(self.path.trim_start_matches('/')).clean()
    }

    fn is_in_diary(path: &str) -> bool {
        clean(path).contains("/diary/")
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

    fn get_relative_link_to(&self, renamed_link_full_path: &str) -> Option<PathBuf> {
        // Strip file extension
        let renamed_link_full_path = PathBuf::from(renamed_link_full_path).with_extension("");
        diff_paths(renamed_link_full_path, &self.content_path)
    }

    fn get_renamed_diary_link(&self, renamed_link_full_path: &str) -> Option<PathBuf> {
        // Strip file extension
        let renamed_link_full_path = PathBuf::from(renamed_link_full_path).with_extension("");
        let mut root = self.wiki_root.to_path_buf();
        root.push("diary");
        renamed_link_full_path
            .strip_prefix(root)
            .map(PathBuf::from)
            .ok()
    }

    fn is_equal_link(&self, link: Link, compare_to_link_full_path: &str) -> bool {
        let compare_to_link = PathBuf::from(compare_to_link_full_path);
        let mut link_path = link.get_full_path(&self.wiki_root, &self.content_path);
        if let Some(extension) = compare_to_link.extension() {
            link_path = link_path.with_extension(extension);
        }
        link_path == compare_to_link
    }

    fn replace_links(
        &self,
        content: &str,
        old_link_full_path: &str,
        renamed_link_full_path: &str,
    ) -> String {
        let relative_path_to_renamed_link = self
            .get_relative_link_to(dbg!(renamed_link_full_path))
            .expect("Should get renamed relative link PathBuf");
        let relative_path_to_renamed_link = dbg!(relative_path_to_renamed_link
            .to_str()
            .expect("Should get renamed relative link path str"));
        let replace = |caps: &Captures| {
            let prefix = caps.name("prefix").map(|m| m.as_str());
            let path = &caps.name("path").expect("Should captured with name link");
            let left = &caps
                .name("left")
                .expect("Should captured with left side of link");
            let right = &caps
                .name("right")
                .expect("Should captured with right side of link");
            if self.is_equal_link(Link::new(prefix, path.as_str()), old_link_full_path) {
                format!(
                    "{}{}{}{}",
                    dbg!(left.as_str()),
                    prefix
                        .map(|s| {
                            let mut s = s.to_owned();
                            s.push(':');
                            s
                        })
                        .unwrap_or("".to_owned()),
                    dbg!(relative_path_to_renamed_link),
                    dbg!(right.as_str())
                )
            } else {
                caps[0].to_owned()
            }
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
    fn it_get_diary_link_full_path() {
        let link = Link::new(Some("diary"), "2020-01-01");
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/index.md");
        assert_eq!(
            link.get_full_path(&wiki_root, &content_path),
            PathBuf::from("/dropbox/vimwiki/diary/2020-01-01")
        );
    }

    #[test]
    fn it_generate_local_file_plain_links() {
        let link = Link::new(Some("diary"), "2020-01-01");
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/index.md");
        assert_eq!(
            Link::new(Some("local"), "images/screen.png").get_full_path(&wiki_root, &content_path),
            PathBuf::from("/dropbox/vimwiki/images/screen.png")
        );
        assert_eq!(
            Link::new(Some("file"), "images/screen.png").get_full_path(&wiki_root, &content_path),
            PathBuf::from("/dropbox/vimwiki/images/screen.png")
        );
        assert_eq!(
            Link::new(None, "note.md").get_full_path(&wiki_root, &content_path),
            PathBuf::from("/dropbox/vimwiki/note.md")
        );
    }

    #[test]
    fn it_generate_with_absolute_link() {
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/index.md");
        assert_eq!(
            Link::new(None, "/note.md").get_full_path(&wiki_root, &content_path),
            PathBuf::from("/dropbox/vimwiki/note.md")
        );
    }

    #[test]
    fn it_should_check_equal_to_full_link_path() {
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/books/index.md");
        let link_factory = Wiki::new(wiki_root, content_path);
        assert!(
            link_factory.is_equal_link(Link::new(None, "../note.md"), "/dropbox/vimwiki/note.md")
        );
        assert!(link_factory.is_equal_link(Link::new(None, "../note"), "/dropbox/vimwiki/note.md"));
    }

    #[test]
    fn it_get_link_relative_to_content() {
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/books/index.md");
        let link_factory = Wiki::new(wiki_root, content_path);
        assert_eq!(
            link_factory.get_relative_link_to("/dropbox/vimwiki/note.md"),
            Some(PathBuf::from("../../note"))
        );
    }

    #[test]
    fn it_replace_diary_links() {
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        // let content_path = PathBuf::from("/dropbox/vimwiki/diary/yyyy-mm-dd.md");
        let content_path = PathBuf::from("/dropbox/vimwiki/books/note.md");
        let link_factory = Wiki::new(wiki_root, content_path);
        let content = r#"
        Here is a [diary](diary:2010-01-01).
        rename to whatever
        "#;
        assert_eq!(
            link_factory.replace_links(
                content,
                "/dropbox/vimwiki/diary/2010-01-01.md",
                "/dropbox/vimwiki/diary/2020-02-02.md"
            ),
            r#"
        Here is a [diary](diary:2020-02-02).
        rename to whatever
        "#
        );
    }
}
