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
