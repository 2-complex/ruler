use std::collections::BTreeMap;

#[derive(Debug, Eq, PartialEq, PartialOrd, Ord)]
enum PathNodeType
{
    Parent(PathBundle),
    Leaf,
}

#[derive(Debug, Eq, PartialEq, PartialOrd, Ord)]
struct PathNode
{
    name : String,
    node_type : PathNodeType,
}

impl PathNode
{
    fn parent(name : String, children : PathBundle) -> Self
    {
        Self{name:name, node_type:PathNodeType::Parent(children)}
    }

    fn leaf(name : String) -> Self
    {
        Self{name:name, node_type:PathNodeType::Leaf}
    }
}

#[derive(Debug, Eq, PartialEq, PartialOrd, Ord)]
struct PathBundle
{
    nodes : Vec<PathNode>
}

fn indented(line: &str) -> Option<&str>
{
    let  mut iter = line.chars();
    match iter.next()
    {
        Some('\t') => Some(iter.as_str()),
        _ => None,
    }
}

#[derive(Debug, PartialEq)]
enum ParseError
{
    Weird,
    Empty,
    DoesNotEndWithNewline,
    ContainsEmptyLines(Vec<usize>),
    Contradiction(usize, usize),
    WrongIndent
}

fn add_to_nodes(nodes : &mut BTreeMap<String, (PathNodeType, usize)>, in_node : PathNode, in_index : usize) -> Result<(), ParseError>
{
    match nodes.get(&in_node.name)
    {
        Some((node_type, index)) =>
        {
            if *node_type != in_node.node_type
            {
                return Err(ParseError::Contradiction(*index, in_index));
            }
        },
        None =>
        {
            nodes.insert(in_node.name, (in_node.node_type, in_index));
        },
    }
    Ok(())
}

impl PathBundle
{
    fn from_lines(lines : Vec<(usize, &str)>) -> Result<Self, ParseError>
    {
        let mut it = lines.iter();
        let mut prev_line = match it.next()
        {
            Some(line) => line,
            None => return Err(ParseError::Weird),
        };

        match indented(prev_line.1)
        {
            Some(_) => return Err(ParseError::WrongIndent),
            None => {}
        }

        let mut nodes = BTreeMap::new();
        while let Some(curr_line) = it.next()
        {
            match indented(curr_line.1)
            {
                None =>
                {
                    add_to_nodes(&mut nodes, PathNode::leaf(prev_line.1.to_string()), prev_line.0)?;
                    prev_line = curr_line;
                },
                Some(rest) =>
                {
                    let mut temp = vec![(curr_line.0, rest)];
                    while let Some(line) = it.next()
                    {
                        match indented(line.1)
                        {
                            None =>
                            {
                                temp.push((0, "")); // oof
                                add_to_nodes(&mut nodes,
                                    PathNode::parent(prev_line.1.to_string(), PathBundle::from_lines(temp)?),
                                    prev_line.0
                                )?;
                                prev_line = line;
                                break;
                            },
                            Some(rest) => temp.push((line.0, rest)),
                        }
                    }
                },
            }
        }

        Ok(PathBundle{nodes:nodes.into_iter().map(
            |(name, (node_type, _index))| {PathNode{name:name, node_type:node_type}}
        ).collect()})
    }

    fn parse(text: &str) -> Result<PathBundle, ParseError>
    {
        match text.chars().last()
        {
            None => return Err(ParseError::Empty),
            Some('\n') => {},
            _ => return Err(ParseError::DoesNotEndWithNewline),
        }

        let lines = text.split('\n').collect::<Vec<&str>>();

        let empty_lines : Vec<usize> = lines[0..lines.len()-1].iter().enumerate().filter(
            |(_, line)| !line.chars().any(|c| c != '\t')).map(|(i, _)| i).collect();

        if empty_lines.len() > 0
        {
            return Err(ParseError::ContainsEmptyLines(empty_lines));
        }

        PathBundle::from_lines(lines.into_iter().enumerate().collect())
    }

    fn get_path_strings_with_prefix(&self, prefix : String, separator : &str) -> Vec<String>
    {
        let mut path_strings = vec![];
        for node in &self.nodes
        {
            match &node.node_type
            {
                PathNodeType::Leaf =>
                    path_strings.push(prefix.clone() + node.name.as_str()),
                PathNodeType::Parent(children) =>
                    path_strings.extend(children.get_path_strings_with_prefix(
                        prefix.clone() + node.name.as_str() + separator, separator)),
            }
        }
        path_strings
    }

    pub fn get_path_strings(&self, separator : char) -> Vec<String>
    {
        self.get_path_strings_with_prefix("".to_string(), separator.to_string().as_str())
    }

    fn get_text_lines(&self, indent : String) -> Vec<String>
    {
        let mut lines = vec![];
        for node in &self.nodes
        {
            lines.push(indent.clone() + node.name.as_str());
            match &node.node_type
            {
                PathNodeType::Leaf => {},
                PathNodeType::Parent(children) =>
                {
                    lines.append(&mut children.get_text_lines(
                        indent.clone() + "\t"));
                }
            }
        }

        lines
    }

    pub fn get_text(&self) -> String
    {
        self.get_text_lines("".to_string()).join("\n") + "\n"
    }
}


#[cfg(test)]
mod test
{
    use crate::bundle::
    {
        PathBundle,
        ParseError,
        PathNode
    };

    /*  Parse an empty string check for the the empty parse-error. */
    #[test]
    fn bundle_parse_empty()
    {
        assert_eq!(PathBundle::parse(""), Err(ParseError::Empty));
    }

    /*  Parse just a newline, check for the ends with empty line parse-error */
    #[test]
    fn bundle_parse_newline()
    {
        assert_eq!(PathBundle::parse("\n"), Err(ParseError::ContainsEmptyLines(vec![0])));
    }

    /*  Parse a bunch of newlines, check for the ends with empty line parse-error */
    #[test]
    fn bundle_parse_newlines()
    {
        assert_eq!(PathBundle::parse("\n\n\n"), Err(ParseError::ContainsEmptyLines(vec![0, 1, 2])));
    }

    /*  Parse a list of files with extra newlines, check for the contains empty error */
    #[test]
    fn bundle_parse_extra_newlines()
    {
        assert_eq!(
            PathBundle::parse("\n\nfile1\nfile2\n"),
            Err(ParseError::ContainsEmptyLines(vec![0, 1])));
    }

    /*  Parse an enindented empty line, check for the empty lines error */
    #[test]
    fn bundle_parse_indented_empty_line()
    {
        assert_eq!(PathBundle::parse("\t\n"), Err(ParseError::ContainsEmptyLines(vec![0])));
    }

    /*  Parse an enindented empty line, check for the empty lines error */
    #[test]
    fn bundle_parse_just_tab()
    {
        assert_eq!(PathBundle::parse("\t"), Err(ParseError::DoesNotEndWithNewline));
    }

    /*  Parse one file except we forgot the newline character at the end */
    #[test]
    fn bundle_parse_one_file_no_newline()
    {
        assert_eq!(PathBundle::parse("file"), Err(ParseError::DoesNotEndWithNewline));
    }

    /*  Parse one file, that should be okay */
    #[test]
    fn bundle_parse_one_file_with_newline()
    {
        PathBundle::parse("file\n").unwrap();
    }

    /*  Parse one file, that should be okay */
    #[test]
    fn bundle_parse_one_file()
    {
        assert_eq!(
            PathBundle::parse("file\n").unwrap(),
            PathBundle{nodes:vec![PathNode::leaf("file".to_string())]})
    }

    /*  Parse one directory with one file in it */
    #[test]
    fn bundle_parse_one_directory()
    {
        assert_eq!(
            PathBundle::parse("directory\n\tfile\n").unwrap(),
            PathBundle{nodes:vec![
                PathNode::parent("directory".to_string(),
                    PathBundle{nodes:vec![PathNode::leaf("file".to_string())]})]});
    }

    /*  Parse one directory with two files in it */
    #[test]
    fn bundle_parse_one_directory_two_files()
    {
        assert_eq!(
            PathBundle::parse("directory\n\tfile1\n\tfile2\n").unwrap(),
            PathBundle{nodes:vec![
                PathNode::parent("directory".to_string(),
                    PathBundle{nodes:vec![
                        PathNode::leaf("file1".to_string()),
                        PathNode::leaf("file2".to_string())]})]});
    }

    /*  Parse two files, check the result */
    #[test]
    fn bundle_parse_two_files()
    {
        assert_eq!(
            PathBundle::parse("file1\nfile2\n").unwrap(),
            PathBundle{nodes:vec![
                PathNode::leaf("file1".to_string()),
                PathNode::leaf("file2".to_string())]});
    }

    /*  Parse two duplicate files, check the result contains only one */
    #[test]
    fn bundle_parse_duplicate_files()
    {
        assert_eq!(
            PathBundle::parse("file\nfile\n").unwrap(),
            PathBundle{nodes:vec![
                PathNode::leaf("file".to_string())]});
    }

    /*  Parse two directories, check the result contains them both */
    #[test]
    fn bundle_parse_two_directories()
    {
        assert_eq!(
            PathBundle::parse("images\n\tapple.png\nproduce\n\tcarrot\n").unwrap(),
            PathBundle{nodes:vec![
                PathNode::parent("images".to_string(),
                    PathBundle{nodes:vec![PathNode::leaf("apple.png".to_string())]}),
                PathNode::parent("produce".to_string(),
                    PathBundle{nodes:vec![PathNode::leaf("carrot".to_string())]}),
            ]});
    }

    /*  Parse two directories, check the result contains them both */
    #[test]
    fn bundle_parse_expect_alphabetical_despite_type()
    {
        assert_eq!(
            PathBundle::parse("images\n\tapple.png\nhenry\njack\n").unwrap(),
            PathBundle{nodes:vec![
                PathNode::leaf("henry".to_string()),
                PathNode::parent("images".to_string(),
                    PathBundle{nodes:vec![PathNode::leaf("apple.png".to_string())]}),
                PathNode::leaf("jack".to_string()),
            ]});
    }

    /*  Parse two directories, check the result contains them both */
    #[test]
    fn bundle_parse_duplicate_directories()
    {
        assert_eq!(
            PathBundle::parse("images\n\tapple.png\nimages\n\tapple.png\n").unwrap(),
            PathBundle{nodes:vec![
                PathNode::parent("images".to_string(),
                    PathBundle{nodes:vec![PathNode::leaf("apple.png".to_string())]}),
            ]});
    }

    /*  Parse two directories with contradicting contents, check for the contradiction error
        with the contradicting lines flagged */
    #[test]
    fn bundle_parse_directories_with_contradictory_content_basic()
    {
        assert_eq!(
            PathBundle::parse("\
images
\tapple.png
images
\tbanana.png
"),
            Err(ParseError::Contradiction(0, 2))
        );
    }

    /*  Parse two directories with contradicting contents, check for the contradiction error
        with the contradicting lines flagged */
    #[test]
    fn bundle_parse_directories_with_contradictory_content_one_deep()
    {
        assert_eq!(
            PathBundle::parse("\
documents
\timages
\t\tapple.png
\timages
\t\tbanana.png
"),
            Err(ParseError::Contradiction(1, 3))
        );
    }

    /*  Parse a directory and a file with the same name, check for contradiction error */
    #[test]
    fn bundle_parse_directory_and_file_same_name()
    {
        assert_eq!(PathBundle::parse("\
produce
\tapple
\tbanana
produce
"),
        Err(ParseError::Contradiction(0, 3)));
    }

    /*  Parse something with wrong indentation, check for the wrong-indentation error */
    #[test]
    fn bundle_parse_wrong_indentation()
    {
        assert_eq!(PathBundle::parse("\t\tapple\n"), Err(ParseError::WrongIndent));
        assert_eq!(PathBundle::parse("produce\n\t\tapple\n"), Err(ParseError::WrongIndent));
    }

    /*  Parse, then get filepaths, and check the result */
    #[test]
    fn bundle_parse_then_get_paths()
    {
        let text = "\
produce
\tapple
\tbanana
images
\tdog.jpg
\tcat.jpg
";
        assert_eq!(PathBundle::parse(text).unwrap().get_path_strings('/'),
            ["images/cat.jpg", "images/dog.jpg", "produce/apple", "produce/banana"]);
    }

    /*  Parse, then get filepaths, and check the result, this time with a different separator */
    #[test]
    fn bundle_parse_then_get_paths_with_backslash()
    {
        let text = "\
produce
\tapple
\tbanana
images
\tdog.jpg
\tcat.jpg
";
        assert_eq!(PathBundle::parse(text).unwrap().get_path_strings('\\'),
            ["images\\cat.jpg", "images\\dog.jpg", "produce\\apple", "produce\\banana"]);
    }

    /*  Parse, then get filepaths, and check the result */
    #[test]
    fn bundle_parse_then_get_paths_deeper()
    {
        let text = "\
a
\ta
\t\ta
\t\t\ta
b
\tb
\t\tb
\t\t\tb
";
        assert_eq!(PathBundle::parse(text).unwrap().get_path_strings('/'), ["a/a/a/a", "b/b/b/b"]);
    }

    /*  Parse, then get filepaths, and check the result */
    #[test]
    fn bundle_parse_then_get_paths_with_redundancy()
    {
        let text = "\
produce
\tapple
\tbanana
produce
\tbanana
\tapple
";
        assert_eq!(PathBundle::parse(text).unwrap().get_path_strings('/'),
            ["produce/apple", "produce/banana"]);
    }

    /*  Roundtrip using parse and get_text */
    #[test]
    fn bundle_parse_roundtrip()
    {
        let text = "\
images
\tcat.jpg
\tdog.jpg
produce
\tapple
\tbanana
";
        assert_eq!(PathBundle::parse(text).unwrap().get_text(), text);
    }

    /*  Roundtrip using parse and get_text */
    #[test]
    fn bundle_parse_roundtrip_more()
    {
        let text = "\
images
\tanimals
\t\tcat.jpg
\t\tdog.jpg
produce
\tfruit
\t\tapple
\t\tbanana
\tveg
\t\tcelery
\t\tlettuce
";
        assert_eq!(PathBundle::parse(text).unwrap().get_text(), text);
    }

    /*  Roundtrip using parse and get_text */
    #[test]
    fn bundle_parse_roundtrip_lots_of_testing()
    {
        let text = "\
a
\ta
\t\ta
\t\t\ta
\t\t\t\ta
\t\t\t\t\ta
\t\t\t\t\t\ta
\t\t\t\t\t\t\ta
\t\t\t\t\t\t\t\ta
\t\t\t\t\t\t\t\t\ta
";

        assert_eq!(PathBundle::parse(text).unwrap().get_text(), text);
    }

    /*  Roundtrip using parse and get_text */
    #[test]
    fn bundle_parse_roundtrip_lots_at_the_base_level()
    {
        let text = "\
apple
blue
lines
link
peach
pizza
rock
sorted
wacky
zebra
";

        assert_eq!(PathBundle::parse(text).unwrap().get_text(), text);
    }

    /*  Roundtrip using parse and get_text.  Check that an unsorted bundle
        round-trips to a sorted one */
    #[test]
    fn bundle_parse_roundtrip_sorts()
    {
        let text_out_of_order = "\
produce
\tveg
\t\tlettuce
\t\tcelery
\tfruit
\t\tbanana
\t\tapple
images
\tanimals
\t\tdog.jpg
\t\tcat.jpg
";

        let text_in_order = "\
images
\tanimals
\t\tcat.jpg
\t\tdog.jpg
produce
\tfruit
\t\tapple
\t\tbanana
\tveg
\t\tcelery
\t\tlettuce
";
        assert_eq!(PathBundle::parse(text_out_of_order).unwrap().get_text(), text_in_order);
    }

    /*  Roundtrip using parse and get_text.  Check that a bundle with dupes
        round-trips to a sorted one without dupes */
    #[test]
    fn bundle_parse_roundtrip_dedupes()
    {
        let text_with_dupes = "\
produce
\tveg
\t\tlettuce
\t\tcelery
\tfruit
\t\tbanana
\t\tapple
produce
\tfruit
\t\tapple
\t\tbanana
\tveg
\t\tcelery
\t\tlettuce
images
\tanimals
\t\tcat.jpg
\t\tdog.jpg
images
\tanimals
\t\tdog.jpg
\t\tcat.jpg
file1
file1
";

        let text_without_dupes = "\
file1
images
\tanimals
\t\tcat.jpg
\t\tdog.jpg
produce
\tfruit
\t\tapple
\t\tbanana
\tveg
\t\tcelery
\t\tlettuce
";
        assert_eq!(PathBundle::parse(text_with_dupes).unwrap().get_text(), text_without_dupes);
    }
}
