use std::collections::HashMap;
use std::fmt::Display;

trait Displaying {
    fn is_row_visible(&self, row: &impl Rowable) -> bool;
}

pub trait Rowable {
    fn get_filterable_data(&self) -> &str;
}

#[derive(Debug, Default)]
pub struct Filters {
    active_filter: Option<FilterType>,
    filters: HashMap<FilterType, Filter>,
}

impl Filters {
    pub fn new() -> Self {
        Self {
            active_filter: None,
            filters: HashMap::new(),
        }
    }

    pub fn push_char(&mut self, c: char) {
        if let Some(ref active_filter) = self.active_filter {
            if let Some(filter) = self.filters.get_mut(&active_filter) {
                filter.push(c);
            }
        }
    }

    pub fn pop_char(&mut self) {
        if let Some(ref active_filter) = self.active_filter {
            if let Some(filter) = self.filters.get_mut(&active_filter) {
                filter.pop();
            }
        }
    }

    pub fn create_filter(&mut self, filter: FilterType, name: &str) {
        self.filters.insert(
            filter,
            Filter {
                predicate: String::new(),
                name: name.to_string(),
            },
        );
    }

    pub fn set_filter(&mut self, mode: FilterType) {
        self.active_filter = Some(mode);
    }

    pub fn is_filter_active(&self, mode: FilterType) -> bool {
        let Some(filter) = &self.active_filter else {
            return false;
        };
        mode == *filter
    }

    pub fn clear_filter(&mut self) {
        self.active_filter = None;
    }

    pub fn is_row_visible(&self, row: &impl Rowable, mode: FilterType) -> bool {
        let Some(filter) = self.filters.get(&mode) else {
            return true;
        };
        filter.is_row_visible(row)
    }
}

impl Display for Filters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = vec![];
        for (filter, filter_data) in &self.filters {
            result.push(filter_data.to_string(self.is_filter_active(*filter)));
        }
        write!(f, "{}", result.join("|"))
    }
}

#[derive(Hash, PartialEq, Eq, Ord, PartialOrd, Debug, Copy, Clone)]
pub enum FilterType {
    Tenant,
    Host,
}

#[derive(Debug)]
struct Filter {
    predicate: String,
    name: String,
}

impl Filter {
    pub fn push(&mut self, c: char) {
        self.predicate.push(c);
    }

    pub fn pop(&mut self) {
        self.predicate.pop();
    }
}

impl Filter {
    fn to_string(&self, active: bool) -> String {
        let typing = if active { " [typing]" } else { "" };
        let value = if self.predicate.is_empty() {
            "<none>"
        } else {
            &self.predicate
        };
        format!("{}{typing}: {value}", self.name)
    }
}

impl Displaying for Filter {
    fn is_row_visible(&self, row: &impl Rowable) -> bool {
        row.get_filterable_data().contains(&self.predicate)
    }
}
