use anyhow;
use fehler::throws;
use path_clean::PathClean;
use pathdiff::diff_paths;
use regex::{Captures, Regex};
use std::path::PathBuf;

// Currently not support:
// - Interwiki links
// - Markdown reference-style links
// diary:file:local:

lazy_static! {
    static ref DEFAULT_LINK_RE: Regex =
        Regex::new(r"(?P<left>\[\[)(?P<link>[^#|]+)(?P<right>(#.*)?(\|.*)*\]\])").unwrap();
    static ref WIKI_INCLUDE_RE: Regex =
        Regex::new(r"(?P<left>\{\{)(?P<link>[^#|]+)(?P<right>(#.*)?(\|.*)*\}\})").unwrap();
    static ref MD_LINK_RE: Regex =
        Regex::new(r"(?P<left>\[.*\]\()(?P<link>[^#]+)(?P<right>(#.*)?\))").unwrap();
}

#[allow(dead_code)]
type Error = anyhow::Error;

enum Link {
    Dairy(PathBuf),
    Other(PathBuf),
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

    fn get_parent(&self) -> PathBuf {
        self.content_path
            .parent()
            .expect("Wiki should have a parent")
            .to_path_buf()
    }

    fn get_link(&self, link: &str) -> PathBuf {
        let mut root;
        let link = if link.starts_with("dairy:") {
            root = self.wiki_root.to_path_buf();
            root.push("dairy");
            link.trim_start_matches("dairy:")
        } else {
            root = self.get_parent();
            link.trim_start_matches("file:")
                .trim_start_matches("local:")
        };
        root.join(PathBuf::from(link.trim_start_matches('/')))
            .clean()
    }

    fn get_relative_link_to(&self, renamed_link_full_path: &str) -> Option<PathBuf> {
        // Strip file extension
        let renamed_link_full_path = PathBuf::from(renamed_link_full_path).with_extension("");
        diff_paths(renamed_link_full_path, &self.content_path)
    }

    fn get_renamed_dairy_link(&self, renamed_link_full_path: &str) -> Option<PathBuf> {
        // Strip file extension
        let renamed_link_full_path = PathBuf::from(renamed_link_full_path).with_extension("");
        let mut root = self.wiki_root.to_path_buf();
        root.push("dairy");
        renamed_link_full_path
            .strip_prefix(root)
            .map(PathBuf::from)
            .ok()
    }

    fn is_equal_link(&self, link: &str, compare_to_link_full_path: &str) -> bool {
        let compare_to_link = PathBuf::from(compare_to_link_full_path);
        let mut link = self.get_link(link);
        if let Some(extension) = compare_to_link.extension() {
            link = link.with_extension(extension);
        }
        link == compare_to_link
    }

    fn replace_links(
        &self,
        content: &str,
        old_link_full_path: &str,
        renamed_link_full_path: &str,
    ) -> String {
        let relative_path_to_renamed_link = self
            .get_relative_link_to(dbg!(renamed_link_full_path))
            .expect("Should get renamed relative link");
        let relative_path_to_renamed_link = dbg!(relative_path_to_renamed_link.to_str().unwrap());
        let replace = |caps: &Captures| {
            let link = &caps.name("link").expect("Should captured with name link");
            let left = &caps
                .name("left")
                .expect("Should captured with left side of link");
            let right = &caps
                .name("right")
                .expect("Should captured with right side of link");
            if self.is_equal_link(dbg!(link.as_str()), old_link_full_path) {
                format!(
                    "{}{}{}",
                    dbg!(left.as_str()),
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
        assert_eq!(&cap["link"], "This is a link");
    }

    #[test]
    fn it_capture_plain_link_with_description() {
        let cap = DEFAULT_LINK_RE
            .captures("[[This is a link|Description of the link]]")
            .unwrap();
        assert_eq!(&cap["link"], "This is a link");
    }

    #[test]
    fn it_capture_transclusion_link() {
        let cap = WIKI_INCLUDE_RE
            .captures("{{file:../../images/vimwiki_logo.png}}")
            .unwrap();
        assert_eq!(&cap["link"], "file:../../images/vimwiki_logo.png");
    }
    #[test]
    fn it_capture_transclusion_link_complex() {
        let cap = WIKI_INCLUDE_RE
            .captures(
                r#"{{http://.../vimwiki_logo.png|cool stuff|style="width:150px;height:120px;"}}"#,
            )
            .unwrap();
        assert_eq!(&cap["link"], "http://.../vimwiki_logo.png");
    }
    #[test]
    fn it_capture_markdown_link() {
        let cap = MD_LINK_RE.captures("[Looks like this](URL)").unwrap();
        assert_eq!(&cap["link"], "URL");
    }
    #[test]
    fn it_capture_wiki_link_with_anchors() {
        let cap = DEFAULT_LINK_RE
            .captures("[[This is a link#Tommorrow]]")
            .unwrap();
        assert_eq!(&cap["link"], "This is a link");
    }
    #[test]
    fn it_capture_markdown_link_with_anchors() {
        let cap = MD_LINK_RE
            .captures("[Looks like this](URL#anchor)")
            .unwrap();
        assert_eq!(&cap["link"], "URL");
    }
}

#[cfg(test)]
mod test_link_factory {
    use super::Wiki;
    use anyhow;
    use fehler::throws;
    use std::path::{Path, PathBuf};

    type Error = anyhow::Error;

    fn create_factory() -> Wiki {
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/index.md");
        Wiki::new(wiki_root, content_path)
    }

    #[test]
    fn it_generate_dairy_link() {
        let link_factory = create_factory();
        assert_eq!(
            link_factory.get_link("dairy:2020-01-01"),
            PathBuf::from("/dropbox/vimwiki/dairy/2020-01-01")
        );
    }

    #[test]
    fn it_generate_local_file_plain_links() {
        let link_factory = create_factory();
        assert_eq!(
            link_factory.get_link("local:images/screen.png"),
            PathBuf::from("/dropbox/vimwiki/images/screen.png")
        );
        assert_eq!(
            link_factory.get_link("file:images/screen.png"),
            PathBuf::from("/dropbox/vimwiki/images/screen.png")
        );
        assert_eq!(
            link_factory.get_link("note.md"),
            PathBuf::from("/dropbox/vimwiki/note.md")
        );
    }

    #[test]
    fn it_generate_with_absolute_link() {
        let link_factory = create_factory();
        assert_eq!(
            link_factory.get_link("/note.md"),
            PathBuf::from("/dropbox/vimwiki/note.md")
        );
    }

    #[test]
    fn it_should_check_equal_to_full_link_path() {
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/books/index.md");
        let link_factory = Wiki::new(wiki_root, content_path);
        assert!(link_factory.is_equal_link("../note.md", "/dropbox/vimwiki/note.md"));
        assert!(link_factory.is_equal_link("../note", "/dropbox/vimwiki/note.md"));
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
    fn it_replace_dairy_links() {
        let wiki_root = PathBuf::from("/dropbox/vimwiki");
        let content_path = PathBuf::from("/dropbox/vimwiki/dairy/yyyy-mm-dd.md");
        let link_factory = Wiki::new(wiki_root, content_path);
        let content = r#"
        Here is a [dairy](dairy:2010-01-01).
        rename to whatever
        "#;
        assert_eq!(
            link_factory.replace_links(
                content,
                "/dropbox/vimwiki/dairy/2010-01-01.md",
                "/dropbox/vimwiki/dairy/2020-01-01.md"
            ),
            r#"
        Here is a [dairy](dairy:2020-01-01).
        rename to whatever
        "#
        );
    }
}
