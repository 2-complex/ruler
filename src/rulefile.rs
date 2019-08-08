extern crate regex;

use regex::Regex;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

pub struct Rule<'r>
{
    pub all : &'r str,
    pub targets : Vec<&'r str>,
    pub sources : Vec<&'r str>,
    pub command : Vec<&'r str>,
    pub source_rule_indices : Vec<usize>,
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
                source_rule_indices : Vec::new(),
            })
        }
    }

    assign_source_rules(&mut result);

    Ok(result)
}

fn assign_source_rules(rules : &mut Vec<Rule>)
{
    let mut dep_map : HashMap<String, usize> = HashMap::new();

    for (index, rule) in rules.iter().enumerate()
    {
        for &t in rule.targets.iter()
        {
            dep_map.insert(t.to_string(), index);
        }
    }

    for rule in rules.iter_mut()
    {
        for &s in rule.sources.iter()
        {
            if let Some(i) = dep_map.get(s)
            {
                rule.source_rule_indices.push(*i);
            }
        }
    }
}

pub fn topological_sort<'a>(
    rules : &'a Vec<Rule<'a>>,
    target : &str
    ) -> Vec<&'a Rule<'a>>
{
    let mut dep_map : HashMap<String, &Rule> = HashMap::new();
    let mut visited : HashSet<String> = HashSet::new();

    for rule in rules
    {
        for &t in rule.targets.iter()
        {
            dep_map.insert(t.to_string(), rule);
        }
    }

    let mut accum : Vec<&Rule> = Vec::new();
    let mut buf = VecDeque::new();

    buf.push_back(target);
    buf.push_back(target);

    while let Some(t) = buf.pop_back()
    {
        if let Some(rule) = dep_map.get(t)
        {
            if visited.contains(t)
            {
                accum.push(rule);
            }
            else
            {
                visited.insert(t.to_string());
                for &s in rule.sources.iter().rev()
                {
                    if !visited.contains(s)
                    {
                        buf.push_back(s);
                        buf.push_back(s);
                    }
                }
            }
        }
    }

    accum
}


pub fn topological_sort_indices(
    rules : &Vec<Rule>,
    target : &str
    ) -> Vec<usize>
{
    let mut dep_map : HashMap<String, usize> = HashMap::new();

    for (index, rule) in rules.iter().enumerate()
    {
        for &t in rule.targets.iter()
        {
            dep_map.insert(t.to_string(), index);
        }
    }

    let mut visited : HashSet<String> = HashSet::new();
    let mut accum : Vec<usize> = Vec::new();
    let mut buf = VecDeque::new();

    buf.push_back(target);
    buf.push_back(target);

    while let Some(t) = buf.pop_back()
    {
        if let Some(i) = dep_map.get(t)
        {
            if visited.contains(t)
            {
                accum.push(*i);
            }
            else
            {
                visited.insert(t.to_string());
                for &s in rules[*i].sources.iter().rev()
                {
                    if !visited.contains(s)
                    {
                        buf.push_back(s);
                        buf.push_back(s);
                    }
                }
            }
        }
    }

    accum
}
