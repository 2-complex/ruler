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
    name : String, // note to self write a test that fails if you swap these
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

static INDENT_CHAR : char = '\t';
static FILE_SEPARATOR : &str = "/";

fn indented(line: &str) -> Option<&str>
{
    let  mut iter = line.chars();
    match iter.next()
    {
        Some(c) =>
        {
            if c == INDENT_CHAR
            {
                Some(iter.as_str())
            }
            else
            {
                None
            }
        },
        None => None,
    }
}

#[derive(Debug, PartialEq)]
enum ParseError
{
    Empty,
    ContainsEmptyLines,
    DoesNotEndWithNewline,
    Contradiction
}

fn is_only_indentation(s: &str) -> bool
{
    for c in s.chars()
    {
        if c != INDENT_CHAR
        {
            return false;
        }
    }
    return true;
}

fn add_to_nodes(nodes : &mut BTreeMap<String, PathNodeType>, in_node : PathNode) -> Result<(), ParseError>
{
    match nodes.get(&in_node.name)
    {
        Some(node_type) =>
        {
            if *node_type != in_node.node_type
            {
                return Err(ParseError::Contradiction);
            }
        },
        None =>
        {
            nodes.insert(in_node.name, in_node.node_type);
        },
    }
    Ok(())
}

impl PathBundle
{
    fn from_lines(lines : Vec<&str>) -> Result<Self, ParseError>
    {
        let mut it = lines.iter();
        let mut prev_name = match it.next()
        {
            Some(line) => line,
            None => return Ok(PathBundle{nodes:vec![]}),
        };

        let mut nodes = BTreeMap::new();
        while let Some(line) = it.next()
        {
            match indented(line)
            {
                None =>
                {
                    add_to_nodes(&mut nodes, PathNode::leaf(prev_name.to_string()))?;
                    prev_name = line;
                },
                Some(rest) =>
                {
                    let mut v = vec![rest];
                    while let Some(line) = it.next()
                    {
                        match indented(line)
                        {
                            None=>
                            {
                                v.push("");
                                add_to_nodes(&mut nodes, PathNode::parent(
                                    prev_name.to_string(),
                                    PathBundle::from_lines(v)?))?;
                                prev_name = line;
                                break;
                            },
                            Some(rest)=>
                            {
                                v.push(rest);
                            },
                        }
                    }
                },
            }
        }

        Ok(PathBundle{nodes:nodes.into_iter().map(
            |(key, value)| {PathNode{name:key, node_type:value}}
        ).collect()})
    }

    fn parse(text: &str) -> Result<PathBundle, ParseError>
    {
        if text == ""
        {
            return Err(ParseError::Empty);
        }

        let lines : Vec<&str> = text.split('\n').collect();

        match lines.last()
        {
            None => return Err(ParseError::Empty),
            Some(&"") => {},
            Some(_) => return Err(ParseError::DoesNotEndWithNewline),
        }

        for line in &lines[0..lines.len()-1]
        {
            if is_only_indentation(line)
            {
                return Err(ParseError::ContainsEmptyLines);
            }
        }

        PathBundle::from_lines(lines)
    }

    fn get_path_strings_with_prefix(self, prefix : String) -> Vec<String>
    {
        let mut path_strings = vec![];
        for node in self.nodes
        {
            match node.node_type
            {
                PathNodeType::Leaf =>
                {
                    path_strings.push(prefix.clone() + node.name.as_str());
                },
                PathNodeType::Parent(path_bundle) =>
                {
                    let new_prefix = prefix.clone() + node.name.as_str();
                    path_strings.extend(path_bundle.get_path_strings_with_prefix(new_prefix));
                }
            }
        }
        path_strings
    }
    
    pub fn get_path_strings(self) -> Vec<String>
    {
        let mut path_strings = vec![];
        for node in self.nodes
        {
            match node.node_type
            {
                PathNodeType::Leaf =>
                {
                    path_strings.push(node.name);
                },
                PathNodeType::Parent(path_bundle) =>
                {
                    path_strings.extend(path_bundle.get_path_strings_with_prefix(node.name + FILE_SEPARATOR));
                }
            }
        }
        path_strings
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

    use std::collections::BTreeSet;

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
        assert_eq!(PathBundle::parse("\n"), Err(ParseError::ContainsEmptyLines));
    }

    /*  Parse a bunch of newlines, check for the ends with empty line parse-error */
    #[test]
    fn bundle_parse_newlines()
    {
        assert_eq!(PathBundle::parse("\n\n\n"), Err(ParseError::ContainsEmptyLines));
    }

    /*  Parse a list of files with extra newlines, check for the contains empty error */
    #[test]
    fn bundle_parse_extra_newlines()
    {
        assert_eq!(PathBundle::parse("\n\nfile1\nfile2\n"), Err(ParseError::ContainsEmptyLines));
    }

    /*  Parse an enindented empty line, check for the empty lines error */
    #[test]
    fn bundle_parse_indented_empty_line()
    {
        assert_eq!(PathBundle::parse("\t\n"), Err(ParseError::ContainsEmptyLines));
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
    fn bundle_parse_duplicate_directories()
    {
        assert_eq!(
            PathBundle::parse("images\n\tapple.png\nimages\n\tapple.png\n").unwrap(),
            PathBundle{nodes:vec![
                PathNode::parent("images".to_string(),
                    PathBundle{nodes:vec![PathNode::leaf("apple.png".to_string())]}),
            ]});
    }

    /*  Parse two directories, check the result contains them both */
    #[test]
    fn bundle_parse_directories_with_contradictory_content()
    {
        assert_eq!(
            PathBundle::parse("images\n\tapple.png\nimages\n\tbanana.png\n"),
            Err(ParseError::Contradiction)
        );
    }

    /*  Put more than one file with the same name into a set and make sure it removes the dupes */
    #[test]
    fn bundle_path_node_set_dedupes_files()
    {
        let mut file_set = BTreeSet::new();
        file_set.insert(PathNode::leaf("carrot".to_string()));
        file_set.insert(PathNode::leaf("carrot".to_string()));
        file_set.insert(PathNode::leaf("carrot".to_string()));
        assert_eq!(file_set.len(), 1);

        let files : Vec<PathNode> = file_set.into_iter().collect();
        assert_eq!(files, vec![PathNode::leaf("carrot".to_string())]);
    }

    /*  Put files not in order into a set and check that they come out in order */
    #[test]
    fn bundle_path_node_set_puts_files_in_order()
    {
        let mut file_set = BTreeSet::new();
        file_set.insert(PathNode::leaf("banana".to_string()));
        file_set.insert(PathNode::leaf("celery".to_string()));
        file_set.insert(PathNode::leaf("apple".to_string()));
        assert_eq!(file_set.len(), 3);

        let files : Vec<PathNode> = file_set.into_iter().collect();
        assert_eq!(files, vec![
            PathNode::leaf("apple".to_string()),
            PathNode::leaf("banana".to_string()),
            PathNode::leaf("celery".to_string()),
        ]);
    }
}



