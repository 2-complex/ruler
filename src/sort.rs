use std::collections::BTreeSet;
use std::collections::HashMap;

use crate::rule::Rule;

/*  Assuming source leaf paths and nodes come in two separate lists,
    this enum encodes a reference by index into one of those lists. */
#[derive(Debug, PartialEq)]
pub enum SourceReference
{
    Leaf(usize),
    Rule(usize),
}

#[derive(Debug, PartialEq)]
pub struct Node
{
    pub targets: Vec<String>,
    pub sources: Vec<SourceReference>,
    pub command : Vec<String>,
    // pub rule_ticket : Ticket,
}

#[derive(Debug, PartialEq)]
pub struct RuleSortResult
{
    leaves : Vec<String>,
    nodes : Vec<Node>
}

fn get_target_to_node_index(rules : &Vec<Rule>) -> HashMap<String, usize>
{
    let mut target_to_node_index = HashMap::new();
    for (index, rule) in rules.iter().enumerate()
    {
        for target in &rule.targets
        {
            target_to_node_index.insert(target.to_string(), index);
        }
    }
    target_to_node_index
}

/*  Construct this object, consuming a vector of Rules, to get a vector of source-paths (called leaves)
    and a vector of Nodes.  Nodes correspond to Rules.

    In the returned struct, "leaves" is a sorted vec of paths which do not occur in the target section
    of any Rule in the input.
*/
impl RuleSortResult
{
    fn new(rules : Vec<Rule>) -> RuleSortResult
    {
        let mut leaves_set = BTreeSet::new();

        let target_to_node_index = get_target_to_node_index(&rules);

        let mut nodes = vec![];
        for rule in rules.into_iter()
        {
            let mut node_sources = vec![];
            for source in rule.sources.into_iter()
            {
                match target_to_node_index.get(&source)
                {
                    None => 
                    {
                        leaves_set.insert(source);
                    },
                    Some(index) =>
                    {
                        node_sources.push(SourceReference::Rule(*index));
                    },
                }
            }

            nodes.push(Node{
                targets: rule.targets,
                sources: node_sources,
                command: rule.command,
            });
        }

        RuleSortResult
        {
            leaves : leaves_set.into_iter().map(|key|{key.to_string()}).collect(),
            nodes : nodes
        }
    }
}

#[cfg(test)]
mod tests
{
    use crate::rule::Rule;
    use crate::sort::
    {
        RuleSortResult,
        Node,
        SourceReference
    };

    /*  Sort an empty rule vector and check for an empty result */
    #[test]
    fn sort_empty()
    {
        assert_eq!(RuleSortResult::new(vec![]),
            RuleSortResult
            {
                leaves: vec![],
                nodes: vec![],
            }
        );
    }

    /*  Sort one rule, check the result */
    #[test]
    fn sort_one_source()
    {
        assert_eq!(
            RuleSortResult::new(vec![
                Rule
                {
                    sources: vec!["apple.c".to_string()],
                    targets: vec!["apple.o".to_string()],
                    command: vec!["compile apple.c to apple.o".to_string()],
                }
            ]),
            RuleSortResult
            {
                leaves: vec!["apple.c".to_string()],
                nodes: vec![
                    Node
                    {
                        sources:vec![SourceReference::Leaf(0)],
                        targets: vec!["apple.o".to_string()],
                        command: vec!["compile apple.c to apple.o".to_string()],
                    }
                ],
            }
        );
    }


}
