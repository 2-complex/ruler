extern crate regex;

use regex::Regex;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;

pub struct Rule<'r>
{
    pub all : &'r str,
    pub targets : Vec<&'r str>,
    pub sources : Vec<&'r str>,
    pub command : Vec<&'r str>,
}

impl<'r> fmt::Display for Rule<'r>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        for t in self.targets.iter()
        {
            write!(f, "{}\n", t).unwrap();
        }
        write!(f, ":\n").unwrap();
        for t in self.sources.iter()
        {
            write!(f, "{}\n", t).unwrap();
        }
        write!(f, ":\n").unwrap();
        for t in self.command.iter()
        {
            write!(f, "{}\n", t).unwrap();
        }
        write!(f, ":\n")
    }
}


pub fn parse(content: &String) -> Result<Vec<Rule>, String>
{
    let mut result = Vec::new();

    let big_re = Regex::new(r"([^\n:][^:]*\n:\n[^\n:][^:]*\n:\n[^\n:][^:]*\n:\n)").unwrap();

    for big_caps in big_re.captures_iter(content)
    {
        let rule_re = Regex::new(r"([^\n:][^:]*)\n:\n([^\n:][^:]*)\n:\n([^\n:][^:]*)\n:\n").unwrap();
        let all = big_caps.get(1).unwrap().as_str();

        for caps in rule_re.captures_iter(all)
        {
            result.push(Rule{
                all : all,
                targets : caps.get(1).unwrap().as_str().lines().collect(),
                sources : caps.get(2).unwrap().as_str().lines().collect(),
                command : caps.get(3).unwrap().as_str().lines().collect(),
            })
        }
    }

    Ok(result)
}

pub fn topological_sort<'a>(
    mut rules : Vec<Rule<'a>>,
    target : &str
    ) -> (Vec<Rule<'a>>, HashMap<String, usize>)
{
    let mut new_rules : HashMap<usize, Rule> = HashMap::new();
    let mut l_index : usize = 0;
    let mut buf : Vec<&str> = Vec::new();

    let mut dep_map : HashMap<String, usize> = HashMap::new();
    while let Some(rule) = rules.pop()
    {
        for &t in rule.targets.iter()
        {
            if t==target
            {
                buf.push(t);
                buf.push(t);
            }

            dep_map.insert(t.to_string(), l_index);
        }
        new_rules.insert(l_index, rule);
        l_index += 1;
    }

    let mut visited : HashSet<String> = HashSet::new();
    let mut rules_in_order : Vec<Rule> = Vec::new();
    let mut source_to_index : HashMap<String, usize> = HashMap::new();

    let mut r_index : usize = 0;

    while let Some(path) = buf.pop()
    {
        match dep_map.get(path)
        {
            Some(i) =>
            {
                if visited.contains(path)
                {
                    rules_in_order.push(new_rules.remove(i).unwrap());
                    source_to_index.insert(path.to_string(), r_index);
                    r_index+=1;
                }
                else
                {
                    visited.insert(path.to_string());
                    let rule = new_rules.get(i).unwrap();

                    for &source in rule.sources.iter().rev()
                    {
                        if !visited.contains(source)
                        {
                            buf.push(source);
                            buf.push(source);
                        }
                    }
                }
            },
            None =>
            {
                if visited.contains(path)
                {
                    rules_in_order.push(
                        Rule
                        {
                            all: "",
                            sources: vec![],
                            targets: vec![path],
                            command: vec![],
                        }
                    );
                    source_to_index.insert(path.to_string(), r_index);
                    r_index+=1;
                }
                else
                {
                    visited.insert(path.to_string());
                }
            }
        }
    }

    (rules_in_order, source_to_index)
}

mod tests {
    use crate::rulefile::topological_sort;  

    #[test]
    fn topological_sort_empty_is_empty()
    {
        let (v, m) = topological_sort(vec![], "");
        assert_eq!(v.len(), 0);
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn topological_sort_single_is_single()
    {
        let rulefile = "abc".to_string();

        println!("{}", rulefile[0..1]);
        println!("{}", rulefile[0..1]);
        println!("{}", rulefile[0..1]);

        let (v, m) = topological_sort(
            vec![Rule
            {
                all : rulefile,
                targets = vec![rulefile[0..1]],
                sources = vec![rulefile[1..2]],
                command = vec![rulefile[2..3]],
            }
        ], "a");

        assert_eq!(v.len(), 1);
        assert_eq!(m.len(), 1);
        assert_eq!(m.get().unwrap());
    }
}

