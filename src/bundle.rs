// use crate::system::
// {
//     // System,
//     // SystemError,
//     // ReadWriteError,
// };
// use crate::ticket::
// {
//     // TicketFactory,
//     // Ticket,
// };
// use serde::
// {
//     // Serialize,
//     // Deserialize,
// };
// use std::fmt;
// use std::time::
// {
//     // SystemTimeError
// };

use std::cmp::Ordering;

#[derive(Debug, Eq)]
enum PathNode
{
    Parent(String, PathBundle),
    End(String),
}

impl PathNode
{
    pub fn clean(&mut self)
    {
        match self
        {
            PathNode::End(_) => {},
            PathNode::Parent(_name, path_bundle) =>
            {
                path_bundle.clean();
            }
        }
    }
    
    pub fn name(&self) -> &str
    {
        match self
        {
            PathNode::End(name) => name,
            PathNode::Parent(name, _) => name,
        }
    }
}

impl Ord for PathNode
{
    fn cmp(&self, other: &Self) -> Ordering
    {
        self.name().cmp(&other.name())
    }
}

impl PartialOrd for PathNode
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering>
    {
        Some(self.cmp(other))
    }
}

impl PartialEq for PathNode
{
    fn eq(&self, other: &Self) -> bool
    {
        self.name() == other.name()
    }
}

#[derive(Debug, Eq, PartialEq)]
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

impl PathBundle
{
    fn from_lines(mut lines : Vec<&str>) -> Self
    {
        let mut path_bundle = PathBundle{nodes:vec![]};
        match lines.last()
        {
            Some(last)=>
            {
                if *last != ""
                {
                    lines.push("");
                }
            },
            None=>
            {
                return path_bundle;
            }
        }

        let mut it = lines.iter();
        let mut prev_name = match it.next()
        {
            Some(line) => line,
            None => return path_bundle,
        };
    
        while let Some(line) = it.next()
        {
            match indented(line)
            {
                None=>
                {
                    path_bundle.nodes.push(PathNode::End(prev_name.to_string()));
                    prev_name = line;
                },
                Some(rest)=>
                {
                    let mut v = vec![rest];
                    while let Some(line) = it.next()
                    {
                        match indented(line)
                        {
                            None=>
                            {
                                path_bundle.nodes.push(PathNode::Parent(prev_name.to_string(), PathBundle::from_lines(v)));
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

        path_bundle
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

        Ok(PathBundle::from_lines(lines))
    }

    fn get_path_strings_with_prefix(self, prefix : String) -> Vec<String>
    {
        let mut path_strings = vec![];
        for node in self.nodes
        {
            match node
            {
                PathNode::End(name) =>
                {
                    path_strings.push(prefix.clone() + name.as_str());
                },
                PathNode::Parent(name, path_bundle) =>
                {
                    let new_prefix = prefix.clone() + name.as_str();
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
            match node
            {
                PathNode::End(name) =>
                {
                    path_strings.push(name);
                },
                PathNode::Parent(name, path_bundle) =>
                {
                    path_strings.extend(path_bundle.get_path_strings_with_prefix(name + FILE_SEPARATOR));
                }
            }
        }
        path_strings
    }

    pub fn clean(&mut self)
    {
        self.nodes.sort();

        for node in self.nodes.iter_mut()
        {
            node.clean();
        }
    }
}


#[cfg(test)]
mod test
{
    use crate::bundle::
    {
        PathBundle,
        ParseError
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
        PathBundle::parse("file\n").unwrap();
    }
}



