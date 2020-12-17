use rand::random;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::activation::ActivationKind;
use crate::node::NodeKind;
use genes::{ConnectionGene, NodeGene};
use mutation::MutationKind;

pub mod crossover;
pub mod genes;
pub mod mutation;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Genome {
    inputs: usize,
    outputs: usize,
    connection_genes: Vec<ConnectionGene>,
    node_genes: Vec<NodeGene>,
}

impl Genome {
    pub fn new(inputs: usize, outputs: usize) -> Self {
        let mut node_genes = vec![];

        (0..inputs).for_each(|_| node_genes.push(NodeGene::new(NodeKind::Input)));
        (0..outputs).for_each(|_| node_genes.push(NodeGene::new(NodeKind::Output)));

        let connection_genes: Vec<ConnectionGene> = (0..inputs)
            .flat_map(|i| {
                (inputs..inputs + outputs)
                    .map(|o| ConnectionGene::new(i, o))
                    .collect::<Vec<ConnectionGene>>()
            })
            .collect();

        Genome {
            inputs,
            outputs,
            connection_genes,
            node_genes,
        }
    }

    fn empty(inputs: usize, outputs: usize) -> Self {
        Genome {
            inputs,
            outputs,
            connection_genes: vec![],
            node_genes: vec![],
        }
    }

    pub fn input_count(&self) -> usize {
        self.inputs
    }

    pub fn output_count(&self) -> usize {
        self.outputs
    }

    pub fn nodes(&self) -> &[NodeGene] {
        &self.node_genes
    }

    pub fn connections(&self) -> &[ConnectionGene] {
        &self.connection_genes
    }

    pub fn crossover(a: (&Self, f64), b: (&Self, f64)) -> Self {
        let inputs_count_not_equal = a.0.inputs != b.0.inputs;
        let outputs_count_not_equal = a.0.outputs != b.0.outputs;

        if inputs_count_not_equal || outputs_count_not_equal {
            panic!("Cannot cross genomes with different inputs or outputs");
        }

        let genome_a = a.0;
        let fitness_a: f64 = a.1;

        let genome_b = b.0;
        let fitness_b: f64 = b.1;

        let fitnesses_equal = (fitness_a - fitness_b).abs() < f64::EPSILON;

        let node_count = if fitnesses_equal {
            let node_max = usize::max(genome_a.node_genes.len(), genome_b.node_genes.len());
            let node_min = usize::min(genome_a.node_genes.len(), genome_b.node_genes.len());
            let no_difference = node_max - node_min == 0;

            if no_difference {
                node_max
            } else {
                node_min + (random::<usize>() % (node_max - node_min))
            }
        } else if fitness_a > fitness_b {
            genome_a.node_genes.len()
        } else {
            genome_b.node_genes.len()
        };

        let mut genome = Genome::empty(genome_a.inputs, genome_a.outputs);

        // Copy the input nodes
        (0..genome_a.inputs).for_each(|i| {
            genome
                .node_genes
                .push(genome_a.node_genes.get(i).unwrap().clone())
        });

        // Pick hidden nodes
        (genome_a.inputs..node_count - genome_a.outputs).for_each(|i| {
            let node_a = genome_a.node_genes.get(i);
            let node_b = genome_b.node_genes.get(i);

            // If one of the genomes is much shorter, this index might be out of bounds
            if node_a.is_none() || node_b.is_none() {
                genome.node_genes.push(node_a.or(node_b).unwrap().clone());
                return;
            }

            let node_a = node_a.unwrap();
            let node_b = node_b.unwrap();

            let picked_node = match (
                matches!(node_a.kind, NodeKind::Hidden),
                matches!(node_b.kind, NodeKind::Hidden),
            ) {
                (true, false) => node_a.clone(),
                (false, true) => node_b.clone(),
                (true, true) => {
                    if random::<f64>() < 0.5 {
                        node_a.clone()
                    } else {
                        node_b.clone()
                    }
                }
                _ => panic!("Both nodes are not of kind hidden"),
            };

            genome.node_genes.push(picked_node);
        });

        // Pick output nodes
        (0..genome_a.outputs).for_each(|i| {
            genome.node_genes.push(if random::<f64>() < 0.5 {
                let index = genome_a.node_genes.len() - genome_a.outputs + i;
                genome_a.node_genes.get(index).unwrap().clone()
            } else {
                let index = genome_b.node_genes.len() - genome_b.outputs + i;
                genome_b.node_genes.get(index).unwrap().clone()
            });
        });

        // TODO do connections
        let mut is_gene_common: HashMap<usize, bool> = HashMap::new();

        genome_a.connection_genes.iter().for_each(|c| {
            let num = c.innovation_number();

            match is_gene_common.get(&num) {
                None => {
                    is_gene_common.insert(num, false);
                }
                Some(false) => {
                    is_gene_common.insert(num, true);
                }
                _ => {}
            }
        });

        genome_b.connection_genes.iter().for_each(|c| {
            let num = c.innovation_number();

            match is_gene_common.get(&num) {
                None => {
                    is_gene_common.insert(num, false);
                }
                Some(false) => {
                    is_gene_common.insert(num, true);
                }
                _ => {}
            }
        });

        is_gene_common.iter().for_each(|(num, is_common)| {
            let a = genome_a
                .connection_genes
                .iter()
                .find(|c| c.innovation_number() == *num);
            let b = genome_b
                .connection_genes
                .iter()
                .find(|c| c.innovation_number() == *num);

            let picked = if *is_common {
                if random::<f64>() < 0.5 {
                    a.cloned()
                } else {
                    b.cloned()
                }
            } else {
                match (fitness_a > fitness_b, a.is_some()) {
                    (true, true) => a.cloned(),
                    (false, false) => b.cloned(),
                    (true, false) => None,
                    (false, true) => None,
                }
            };

            if let Some(conn) = picked {
                genome.connection_genes.push(conn);
            }
        });

        genome.connection_genes.sort_by(|a, b| {
            if a.from == b.from {
                a.to.cmp(&b.to)
            } else {
                a.from.cmp(&b.from)
            }
        });

        genome
    }

    fn get_node_order(
        &self,
        additional_connections: Option<Vec<ConnectionGene>>,
    ) -> Option<Vec<usize>> {
        let mut connections: Vec<ConnectionGene> = self
            .connection_genes
            .iter()
            .filter(|c| !c.disabled)
            .cloned()
            .collect();

        if let Some(mut conns) = additional_connections {
            connections.append(&mut conns);
        }

        let mut visited: Vec<usize> = vec![];

        // Input nodes are automatically visited as they get their values from inputs
        self.node_genes
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.kind, NodeKind::Input))
            .for_each(|(i, _)| {
                visited.push(i);
            });

        let mut newly_visited = 1;
        while newly_visited != 0 {
            newly_visited = 0;

            let mut nodes_to_visit: Vec<usize> = self
                .node_genes
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    // The node is not visited but all prerequisite nodes are visited
                    !visited.contains(i)
                        && connections
                            .iter()
                            .filter(|c| c.to == *i)
                            .map(|c| c.from)
                            .all(|node_index| visited.contains(&node_index))
                })
                .map(|(i, _)| i)
                .collect();

            newly_visited += nodes_to_visit.len();
            visited.append(&mut nodes_to_visit);
        }

        if visited.len() != self.node_genes.len() {
            return None;
        }

        Some(visited)
    }

    pub fn node_order(&self) -> Option<Vec<usize>> {
        self.get_node_order(None)
    }

    pub fn node_order_with(
        &self,
        additional_connections: Vec<ConnectionGene>,
    ) -> Option<Vec<usize>> {
        self.get_node_order(Some(additional_connections))
    }

    fn is_projecting_directly(&self, source: usize, target: usize) -> bool {
        self.connection_genes
            .iter()
            .filter(|c| !c.disabled)
            .any(|c| c.from == source && c.to == target)
    }

    fn is_projected_directly(&self, target: usize, source: usize) -> bool {
        self.is_projecting_directly(source, target)
    }

    fn is_projecting(&self, source: usize, target: usize) -> bool {
        let mut visited_nodes: HashSet<usize> = HashSet::new();
        let mut nodes_to_visit: VecDeque<usize> = VecDeque::new();

        nodes_to_visit.push_back(source);

        let mut projecting = false;
        while let Some(i) = nodes_to_visit.pop_front() {
            visited_nodes.insert(i);
            if self.is_projecting_directly(i, target) {
                projecting = true;
                break;
            } else {
                self.connection_genes
                    .iter()
                    .filter(|c| c.from == i && !c.disabled && !visited_nodes.contains(&i))
                    .for_each(|c| nodes_to_visit.push_back(c.to));
            }
        }

        projecting
    }

    fn is_projected(&self, target: usize, source: usize) -> bool {
        self.is_projecting(source, target)
    }

    fn can_connect(&self, from: usize, to: usize) -> bool {
        let from_node = self.node_genes.get(from).unwrap();
        let to_node = self.node_genes.get(to).unwrap();

        let is_from_output = matches!(from_node.kind, NodeKind::Output);
        let is_to_input = matches!(to_node.kind, NodeKind::Input);

        if is_from_output
            || is_to_input
            || self
                .node_order_with(vec![ConnectionGene::new(from, to)])
                .is_none()
        {
            false
        } else {
            !self.is_projecting(from, to)
        }
    }

    fn add_connection(&mut self, from: usize, to: usize) -> Result<usize, ()> {
        if !self.can_connect(from, to) {
            return Err(());
        }

        let maybe_connection = self
            .connection_genes
            .iter_mut()
            .find(|c| c.from == from && c.to == to);

        if let Some(mut conn) = maybe_connection {
            conn.disabled = false;
        } else {
            self.connection_genes.push(ConnectionGene::new(from, to));
        }

        Ok(self.connection_genes.len() - 1)
    }

    fn add_many_connections(&mut self, params: &[(usize, usize)]) -> Vec<Result<usize, ()>> {
        let results = params
            .iter()
            .map(|(from, to)| self.add_connection(*from, *to))
            .collect();

        results
    }

    fn disable_connection(&mut self, index: usize) {
        self.connection_genes.get_mut(index).unwrap().disabled = true;
    }

    fn disable_many_connections(&mut self, indexes: &[usize]) {
        // if indexes.is_empty() {
        //     return;
        // }

        indexes.iter().for_each(|i| self.disable_connection(*i));

        // let mut indexes_copy: Vec<usize> = (*indexes).to_vec();

        // indexes_copy.sort_unstable();
        // indexes_copy.dedup();

        // indexes_copy.iter().rev().for_each(|i| {
        //     self.connection_genes.remove(*i);
        // });
    }

    /// Add a new hidden node to the genome
    fn add_node(&mut self) -> usize {
        let index = self.node_genes.len();
        self.node_genes.push(NodeGene::new(NodeKind::Hidden));

        index
    }

    // fn remove_node(&mut self, index: usize) {
    //     if !matches!(self.node_genes.get(index).unwrap().kind, NodeKind::Hidden) {
    //         panic!("Cannot remove a non hidden node");
    //     }

    //     self.node_genes.remove(index);
    //     self.connection_genes.iter_mut().for_each(|c| {
    //         if c.from > index {
    //             c.from -= 1;
    //         }

    //         if c.to > index {
    //             c.to -= 1;
    //         }
    //     });
    // }

    pub fn mutate(&mut self) {
        let kind: MutationKind = random();
        mutation::mutate(kind, self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize() {
        Genome::new(2, 2);
    }

    #[test]
    fn add_node_does_not_change_connections() {
        let mut g = Genome::new(1, 2);

        g.add_node();

        let first_connection = g.connection_genes.get(0).unwrap();
        assert_eq!(first_connection.from, 0);
        assert_eq!(first_connection.to, 1);

        let second_connection = g.connection_genes.get(1).unwrap();
        assert_eq!(second_connection.from, 0);
        assert_eq!(second_connection.to, 2);
    }

    #[test]
    fn crossover() {
        let a = Genome::new(2, 2);
        let b = Genome::new(2, 2);

        Genome::crossover((&a, 1.), (&b, 2.));
    }

    #[test]
    #[should_panic]
    fn crossover_fail_1() {
        let a = Genome::new(2, 3);
        let b = Genome::new(2, 2);

        Genome::crossover((&a, 1.), (&b, 2.));
    }

    #[test]
    #[should_panic]
    fn crossover_fail_2() {
        let a = Genome::new(3, 2);
        let b = Genome::new(2, 2);

        Genome::crossover((&a, 1.), (&b, 2.));
    }

    #[test]
    fn is_projecting_directly() {
        let g = Genome::new(2, 2);

        assert!(g.is_projecting_directly(0, 2));
        assert!(g.is_projecting_directly(0, 3));
        assert!(g.is_projecting_directly(1, 2));
        assert!(g.is_projecting_directly(1, 3));

        assert!(!g.is_projecting_directly(2, 0));
        assert!(!g.is_projecting_directly(3, 0));
        assert!(!g.is_projecting_directly(2, 1));
        assert!(!g.is_projecting_directly(3, 1));
    }

    #[test]
    fn is_projected_directly() {
        let g = Genome::new(2, 2);

        assert!(g.is_projected_directly(2, 0));
        assert!(g.is_projected_directly(3, 0));
        assert!(g.is_projected_directly(2, 1));
        assert!(g.is_projected_directly(3, 1));

        assert!(!g.is_projected_directly(0, 2));
        assert!(!g.is_projected_directly(0, 3));
        assert!(!g.is_projected_directly(1, 2));
        assert!(!g.is_projected_directly(1, 3));
    }

    // TODO rewrite the tests or both the implementation and tests

    // #[test]
    // fn is_projecting() {
    //     let mut g = Genome::empty(1, 1);

    //     g.node_genes.push(NodeGene::new(NodeKind::Input));
    //     g.node_genes.push(NodeGene::new(NodeKind::Hidden));
    //     g.node_genes.push(NodeGene::new(NodeKind::Hidden));
    //     g.node_genes.push(NodeGene::new(NodeKind::Output));

    //     g.connection_genes.push(ConnectionGene::new(0, 1));
    //     g.connection_genes.push(ConnectionGene::new(1, 2));
    //     g.connection_genes.push(ConnectionGene::new(2, 3));

    //     assert!(g.is_projecting(0, 3));
    //     assert!(g.is_projecting(1, 3));
    //     assert!(g.is_projecting(2, 3));

    //     assert!(!g.is_projecting(3, 0));
    //     assert!(!g.is_projecting(3, 1));
    //     assert!(!g.is_projecting(3, 2));
    // }

    // #[test]
    // fn is_projected() {
    //     let mut g = Genome::empty(1, 1);

    //     g.node_genes.push(NodeGene::new(NodeKind::Input));
    //     g.node_genes.push(NodeGene::new(NodeKind::Hidden));
    //     g.node_genes.push(NodeGene::new(NodeKind::Hidden));
    //     g.node_genes.push(NodeGene::new(NodeKind::Output));

    //     g.connection_genes.push(ConnectionGene::new(0, 1));
    //     g.connection_genes.push(ConnectionGene::new(1, 2));
    //     g.connection_genes.push(ConnectionGene::new(2, 3));

    //     assert!(g.is_projected(3, 0));
    //     assert!(g.is_projected(3, 1));
    //     assert!(g.is_projected(3, 2));

    //     assert!(!g.is_projected(0, 3));
    //     assert!(!g.is_projected(1, 3));
    //     assert!(!g.is_projected(2, 3));
    // }

    #[test]
    fn can_connect() {
        let mut g = Genome::empty(1, 1);

        g.node_genes.push(NodeGene::new(NodeKind::Input));
        g.node_genes.push(NodeGene::new(NodeKind::Hidden));
        g.node_genes.push(NodeGene::new(NodeKind::Hidden));
        g.node_genes.push(NodeGene::new(NodeKind::Hidden));
        g.node_genes.push(NodeGene::new(NodeKind::Output));

        g.connection_genes.push(ConnectionGene::new(0, 1));
        g.connection_genes.push(ConnectionGene::new(0, 2));
        g.connection_genes.push(ConnectionGene::new(1, 3));
        g.connection_genes.push(ConnectionGene::new(2, 3));
        g.connection_genes.push(ConnectionGene::new(3, 4));

        assert!(g.can_connect(1, 2));
        assert!(g.can_connect(2, 1));

        assert!(!g.can_connect(3, 1));
        assert!(!g.can_connect(3, 2));
        assert!(!g.can_connect(4, 1));
        assert!(!g.can_connect(4, 2));
    }

    #[test]
    fn get_node_order() {
        let mut g = Genome::empty(2, 1);

        g.node_genes.push(NodeGene::new(NodeKind::Input));
        g.node_genes.push(NodeGene::new(NodeKind::Input));
        g.node_genes.push(NodeGene::new(NodeKind::Output));
        g.node_genes.push(NodeGene::new(NodeKind::Hidden));
        g.node_genes.push(NodeGene::new(NodeKind::Hidden));
        g.node_genes.push(NodeGene::new(NodeKind::Hidden));

        g.add_connection(0, 2).unwrap();
        g.add_connection(1, 3).unwrap();
        g.add_connection(1, 4).unwrap();
        g.add_connection(1, 5).unwrap();
        g.add_connection(3, 2).unwrap();
        g.add_connection(4, 3).unwrap();
        g.add_connection(5, 4).unwrap();

        assert!(g.node_order().is_some());
        assert!(g.node_order_with(vec![ConnectionGene::new(3, 5)]).is_none());
    }

    #[test]
    fn no_recurrent_connections() {
        let mut g = Genome::empty(2, 1);

        g.node_genes.push(NodeGene::new(NodeKind::Input));
        g.node_genes.push(NodeGene::new(NodeKind::Input));
        g.node_genes.push(NodeGene::new(NodeKind::Output));
        g.node_genes.push(NodeGene::new(NodeKind::Hidden));
        g.node_genes.push(NodeGene::new(NodeKind::Hidden));
        g.node_genes.push(NodeGene::new(NodeKind::Hidden));

        g.add_connection(0, 2).unwrap();
        g.add_connection(1, 3).unwrap();
        g.add_connection(1, 4).unwrap();
        g.add_connection(1, 5).unwrap();
        g.add_connection(3, 2).unwrap();
        g.add_connection(4, 3).unwrap();
        g.add_connection(5, 4).unwrap();

        assert!(g.add_connection(3, 5).is_err());
    }
}
