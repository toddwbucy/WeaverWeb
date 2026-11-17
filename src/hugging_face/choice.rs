//! **A choice is a model, not a file** (Spec 6.2). Files named
//! `<stem>-NNNNN-of-MMMMM.gguf`, llama.cpp's split naming (`%05d`), with one
//! stem and one count group into one choice listing every part and their
//! total size, offered only where every part from 1 to the count stands at
//! the commit. Every other `.gguf` file is a choice of its own, and a file
//! of another kind is no choice.

use crate::hugging_face::RepoFile;

/// **The split part `path` names**: its stem, its number and the count,
/// each number five digits, the number from 1 to the count. A name whose
/// numbers do not hold is no part, and so a choice of its own.
pub fn part_of(path: &str) -> Option<(&str, u32, u32)> {
    let base = path.strip_suffix(".gguf")?;
    let cut = base.len().checked_sub(15)?;
    if cut == 0 || !base.is_char_boundary(cut) {
        return None;
    }
    let (stem, tail) = base.split_at(cut);
    let tail = tail.as_bytes();
    let digits = |b: &[u8]| b.iter().all(u8::is_ascii_digit);
    if tail[0] != b'-' || &tail[6..10] != b"-of-" || !digits(&tail[1..6]) || !digits(&tail[10..]) {
        return None;
    }
    let number: u32 = std::str::from_utf8(&tail[1..6]).ok()?.parse().ok()?;
    let count: u32 = std::str::from_utf8(&tail[10..]).ok()?.parse().ok()?;
    (1..=count)
        .contains(&number)
        .then_some((stem, number, count))
}

/// One choice of a repository at a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// The choice's name: its first part's path, or the one file's.
    pub name: String,
    /// The parts that stand at the commit, in split order.
    pub parts: Vec<RepoFile>,
    /// The count the file names claim, for a split model.
    pub count: Option<u32>,
    /// Why the choice is offered no pick, where a part does not stand.
    pub missing: Option<String>,
}

impl Choice {
    /// The bytes of every part that stands.
    pub fn bytes(&self) -> u64 {
        self.parts.iter().map(|p| p.size).sum()
    }
}

/// A split model's parts as the listing gives them, keyed by their stem
/// and count, each with its number.
type Groups = Vec<((String, u32), Vec<(u32, RepoFile)>)>;

/// **The choices among `files`**, by name, whatever order the listing gives
/// them in.
pub fn choices(files: &[RepoFile]) -> Vec<Choice> {
    let mut out: Vec<Choice> = Vec::new();
    let mut groups: Groups = Vec::new();
    for file in files.iter().filter(|f| f.path.ends_with(".gguf")) {
        match part_of(&file.path) {
            Some((stem, number, count)) => {
                let key = (stem.to_owned(), count);
                match groups.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, parts)) => parts.push((number, file.clone())),
                    None => groups.push((key, vec![(number, file.clone())])),
                }
            }
            None => out.push(Choice {
                name: file.path.clone(),
                parts: vec![file.clone()],
                count: None,
                missing: None,
            }),
        }
    }
    for ((stem, count), mut parts) in groups {
        parts.sort_by_key(|(number, _)| *number);
        let absent: Vec<String> = (1..=count)
            .filter(|n| !parts.iter().any(|(m, _)| m == n))
            .map(|n| n.to_string())
            .collect();
        let missing = match absent.len() {
            0 => None,
            1 => Some(format!(
                "part {} of {count} does not stand at the commit",
                absent[0]
            )),
            _ => Some(format!(
                "parts {} of {count} do not stand at the commit",
                absent.join(", ")
            )),
        };
        out.push(Choice {
            name: format!("{stem}-00001-of-{count:05}.gguf"),
            parts: parts.into_iter().map(|(_, f)| f).collect(),
            count: Some(count),
            missing,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// **One choice's files in split order**, as the base's identity hashes
/// them: one file that names no part, or every part of one stem and count
/// from 1 to the count, each once, whatever order they are given in.
pub fn in_split_order(files: &[RepoFile]) -> Result<Vec<RepoFile>, String> {
    if files.is_empty() {
        return Err("no file is named".into());
    }
    if let [file] = files
        && part_of(&file.path).is_none()
    {
        return Ok(vec![file.clone()]);
    }
    let mut parts = Vec::new();
    let mut group = None;
    for file in files {
        let (stem, number, count) = part_of(&file.path)
            .ok_or_else(|| format!("{} is not a part of a split model", file.path))?;
        if *group.get_or_insert((stem, count)) != (stem, count) {
            return Err("the files are parts of more than one model".into());
        }
        parts.push((number, file.clone()));
    }
    parts.sort_by_key(|(number, _)| *number);
    let count = group.map_or(0, |(_, count)| count);
    let numbers: Vec<u32> = parts.iter().map(|(n, _)| *n).collect();
    if numbers != (1..=count).collect::<Vec<_>>() {
        return Err(format!("the parts are not each of 1 to {count} once"));
    }
    Ok(parts.into_iter().map(|(_, f)| f).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, size: u64) -> RepoFile {
        RepoFile {
            path: path.to_owned(),
            size,
            blob_id: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            sha256: None,
        }
    }

    /// **A part's name holds five digits each side**, numbered from 1 to
    /// the count, and a name whose numbers do not hold is no part.
    #[test]
    fn a_parts_name_holds_its_numbers() {
        assert_eq!(part_of("m-00002-of-00003.gguf"), Some(("m", 2, 3)));
        assert_eq!(
            part_of("sub/dir/m-q4-00001-of-00002.gguf"),
            Some(("sub/dir/m-q4", 1, 2))
        );
        for no in [
            "m-00000-of-00002.gguf",
            "m-00003-of-00002.gguf",
            "m-0001-of-00002.gguf",
            "m-00001-of-0002.gguf",
            "m-00001-of-00002.bin",
            "-00001-of-00002.gguf",
            "m.gguf",
            "m-0000a-of-00002.gguf",
        ] {
            assert_eq!(part_of(no), None, "{no}");
        }
    }

    /// **Two parts of one stem and count are one choice** with their total
    /// size, in split order whatever the listing's order, a part missing
    /// leaves the choice unoffered with that reason, every other `.gguf`
    /// file is a choice of its own, and another file is none.
    #[test]
    fn parts_group_into_one_choice_and_other_files_stand_alone() {
        let listed = choices(&[
            file("config.json", 2),
            file("m-00002-of-00002.gguf", 30),
            file("lone.gguf", 7),
            file("m-00001-of-00002.gguf", 40),
            file("cut-00003-of-00003.gguf", 1),
            file("cut-00001-of-00003.gguf", 1),
            file("m-00001-of-00003.gguf", 5),
        ]);
        let names: Vec<&str> = listed.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "cut-00001-of-00003.gguf",
                "lone.gguf",
                "m-00001-of-00002.gguf",
                "m-00001-of-00003.gguf",
            ]
        );
        let split = &listed[2];
        assert_eq!(split.bytes(), 70);
        assert_eq!(split.count, Some(2));
        assert_eq!(split.missing, None);
        assert_eq!(
            split
                .parts
                .iter()
                .map(|p| p.path.as_str())
                .collect::<Vec<_>>(),
            ["m-00001-of-00002.gguf", "m-00002-of-00002.gguf"]
        );
        assert_eq!(
            listed[0].missing.as_deref(),
            Some("part 2 of 3 does not stand at the commit")
        );
        assert_eq!(
            listed[3].missing.as_deref(),
            Some("parts 2, 3 of 3 do not stand at the commit")
        );
        assert_eq!(listed[1].count, None);
        assert_eq!(listed[1].missing, None);
    }

    /// **The files in split order**, from any order, and refused where they
    /// are not one model's parts each once.
    #[test]
    fn the_files_are_put_in_split_order_or_refused() {
        let order = in_split_order(&[
            file("m-00002-of-00002.gguf", 1),
            file("m-00001-of-00002.gguf", 1),
        ])
        .unwrap();
        assert_eq!(order[0].path, "m-00001-of-00002.gguf");
        assert_eq!(in_split_order(&[file("lone.gguf", 1)]).unwrap().len(), 1);
        for refused in [
            vec![file("m-00001-of-00002.gguf", 1)],
            vec![
                file("m-00001-of-00002.gguf", 1),
                file("n-00002-of-00002.gguf", 1),
            ],
            vec![file("lone.gguf", 1), file("other.gguf", 1)],
            vec![],
        ] {
            assert!(in_split_order(&refused).is_err(), "{refused:?}");
        }
    }
}
