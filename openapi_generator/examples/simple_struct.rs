use std::collections::HashSet;
struct Simpler {
    id: usize,
}
struct SimpleStruct {
    id: usize,
    name: String,
    optional_value: Option<u64>,
    list_of_values: Vec<char>,
    one_set: HashSet<i32>,
    simpler: Simpler,
}
