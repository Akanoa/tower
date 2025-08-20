//! This module provides a filtering system for handling and interacting with filters.
//! Filters can have an active state, allowing for dynamic manipulation of filtering criteria
//! and visibility checks on data rows based on the applied filters.
use std::collections::HashMap;
use std::fmt::Display;

/// A trait that provides functionality for determining the visibility of a row.
///
/// This trait should be implemented by any type that requires logic
/// to check if a specific row is visible based on custom criteria.
trait Displaying {
    /// Determines whether a given row is visible based on the specific implementation of the `Rowable` trait.
    ///
    /// # Parameters
    /// - `&self`: A reference to the current instance of the object where this method is implemented.
    /// - `row`: A reference to an object that implements the `Rowable` trait. This represents the row
    ///   whose visibility is being checked.
    ///
    /// # Returns
    /// - `bool`: Returns `true` if the row is visible, otherwise returns `false`.
    ///
    /// # Note
    /// The visibility logic is determined by the specific implementation of the `is_row_visible` method in the struct or type
    /// where it is defined. The method relies on the contract defined by the `Rowable` trait.
    ///
    /// The behavior of this method will vary depending on the implementation details
    /// provided for the type `Self` and how it interacts with the `Rowable` trait.
    fn is_row_visible(&self, row: &impl Rowable) -> bool;
}

/// The `Rowable` trait is designed to represent objects that can provide
/// data which can be used for filtering purposes. Implementing this trait
/// allows an object to expose a specific piece of information as a string,
/// which may be utilized in scenarios like searching, sorting, or filtering.
///
/// # Required Methods
///
/// - [`get_filterable_data`]: This function returns a reference to a string
///   slice that represents the data that can be used for filtering.
///
/// This demonstrates that a `User` struct implementing the `Rowable` trait
/// can provide its `username` as the filterable data.
pub trait Rowable {
    /// Retrieves filterable data associated with the implementing instance.
    ///
    /// # Returns
    /// A string slice (`&str`) representing the data that can be used for filtering purposes.
    ///
    /// # Remarks
    /// - The returned data is intended to be used in contexts where filtering functionality is required.
    /// - This method does not modify the state of the implementing instance.
    fn get_filterable_data(&self) -> &str;
}

/// A structure that represents a collection of filters and an optional active filter.
///
/// This structure provides functionality to manage filters, where each filter is associated with a `FilterType`
/// and can be stored in a `HashMap`. Additionally, it tracks the currently active filter if one is set.
///
/// # Fields
///
/// - `active_filter`:
///     An `Option` containing the currently active `FilterType`. If no filter is active, it is `None`.
///
/// - `filters`:
///     A `HashMap` that stores the available filters, where the keys are `FilterType` values and the values
///     are the corresponding `Filter` objects.
///
/// # Derive
/// - `Debug`:
///     Implements the `Debug` trait for debugging purposes, allowing the struct to be formatted using the `{:?}` formatter.
/// - `Default`:
///     Implements the `Default` trait for creating an instance with default values. By default:
///         - `active_filter` is `None`.
///         - `filters` is an empty `HashMap`.
///
/// # Usage
///
/// This struct can be used to manage and query filters in an application, including the ability to:
/// - Set an active filter.
/// - Store multiple filters by their corresponding `FilterType`.
#[derive(Debug, Default)]
pub struct Filters {
    active_filter: Option<FilterType>,
    filters: HashMap<FilterType, Filter>,
}

impl Filters {
    /// Creates a new instance of the struct.
    ///
    /// This function initializes and returns a new instance with the following default values:
    /// - `active_filter`: Set to `None`.
    /// - `filters`: An empty `HashMap`.
    ///
    /// # Returns
    /// A new instance of the struct with default settings.
    ///
    pub fn new() -> Self {
        Self {
            active_filter: None,
            filters: HashMap::new(),
        }
    }

    /// Pushes a character to the active filter's buffer, if an active filter is set and exists in the filters map.
    ///
    /// # Parameters
    /// - `c`: The character to be added to the active filter's buffer.
    ///
    /// # Behavior
    /// - If there is an active filter (`self.active_filter`) and it exists in the `filters` map,
    ///   the provided character `c` is pushed to the corresponding filter's buffer.
    /// - If no active filter is set, or the active filter does not exist in the `filters` map, the function does nothing.
    ///
    /// # Usage
    /// - This function is typically used to dynamically append characters to a specific filter's buffer.
    /// - Assumes that `self.active_filter` and `self.filters` are properly maintained.
    ///
    pub fn push_char(&mut self, c: char) {
        if let Some(ref active_filter) = self.active_filter {
            if let Some(filter) = self.filters.get_mut(&active_filter) {
                filter.push(c);
            }
        }
    }

    /// Removes the last character from the active filter's associated string, if applicable.
    ///
    /// This method operates on a mutable reference to the current instance and modifies
    /// the active filter's stored entry within the `filters` collection.
    ///
    /// ### Behavior:
    /// 1. Checks if there is a currently active filter (`self.active_filter` is `Some`).
    /// 2. Retrieves the mutable reference to the `filter` associated with the active filter key from
    ///    the `filters` collection.
    /// 3. If the key exists in `filters`, calls `pop` on the corresponding filter value (assumes
    ///    the value implements a `pop` method, such as a `String`).
    /// 4. If no active filter or no associated key exists in `filters`, no action is performed.
    ///
    /// ### Note:
    /// - This function does nothing if there is no active filter (`self.active_filter` is `None`).
    /// - This function also does nothing if no entry exists in the `filters` collection for the active filter.
    ///
    pub fn pop_char(&mut self) {
        if let Some(ref active_filter) = self.active_filter {
            if let Some(filter) = self.filters.get_mut(&active_filter) {
                filter.pop();
            }
        }
    }

    /// Creates and adds a new filter to the internal filter collection.
    ///
    /// # Parameters
    /// - `filter`: The `FilterType` that acts as a key for the filter in the internal collection.
    /// - `name`: A string slice (`&str`) representing the name of the filter to be created.
    ///
    /// The method creates a new `Filter` instance with a default empty predicate (`String::new()`)
    /// and the provided `name`. The filter is then inserted into the `filters` collection.
    /// - `FilterType` is expected to be an enum or a similar type.
    /// - The `filters` field is assumed to be a `HashMap` or similar collection where
    ///   the filters are stored.
    pub fn create_filter(&mut self, filter: FilterType, name: &str) {
        self.filters.insert(
            filter,
            Filter {
                predicate: String::new(),
                name: name.to_string(),
            },
        );
    }

    /// Sets the active filter mode for the object.
    ///
    /// This method updates the `active_filter` field of the struct to the specified
    /// `FilterType`. It allows the user to configure the desired filter mode for
    /// further processing or operations.
    ///
    /// # Parameters
    /// - `mode`: The `FilterType` variant representing the filter mode to be applied.
    ///
    /// # Note
    /// Ensure that the `active_filter` is correctly set before calling any dependent
    /// methods that rely on the filter mode.
    pub fn set_filter(&mut self, mode: FilterType) {
        self.active_filter = Some(mode);
    }

    /// Checks if a specific filter is active.
    ///
    /// This method compares the provided `mode` with the current `active_filter`
    /// to determine if the given filter type is the one currently active.
    ///
    /// # Arguments
    ///
    /// * `mode` - A `FilterType` enum value representing the filter type to check.
    ///
    /// # Returns
    ///
    /// * `true` if the provided filter type (`mode`) matches the active filter.
    /// * `false` if no filter is active or if the provided filter type does not match.
    pub fn is_filter_active(&self, mode: FilterType) -> bool {
        let Some(filter) = &self.active_filter else {
            return false;
        };
        mode == *filter
    }

    /// Clears the currently active filter by setting it to `None`.
    ///
    /// This method updates the `active_filter` field of the instance, effectively
    /// removing any previously applied filter. Typically, this is used when no filtering
    /// criteria need to be applied anymore.
    pub fn clear_filter(&mut self) {
        self.active_filter = None;
    }

    /// Determines the visibility of a row based on the applied filter.
    ///
    /// # Parameters
    /// - `row`: A reference to an object that implements the `Rowable` trait.
    ///   Represents the row whose visibility is being checked.
    /// - `mode`: A `FilterType` that defines the filtering mode to be used for
    ///   determining row visibility.
    ///
    /// # Returns
    /// - `true` if the row is visible (either no filter exists for the specified mode
    ///   or the filter permits the row to be visible).
    /// - `false` if the filter for the specified mode deems the row invisible.
    ///
    /// # Behavior
    /// - The function first checks if a filter exists for the given `mode` by looking it up
    ///   in `self.filters`.
    /// - If no filter is associated with the specified `mode`, the function returns `true`,
    ///   making the row visible by default.
    /// - If a filter exists, it delegates to the filter's `is_row_visible` method to determine
    ///   the visibility of the row based on the filtering logic.
    ///
    /// # Note
    /// This function assumes that all filters implement an `is_row_visible` method
    /// to apply their specific filtering rules.
    pub fn is_row_visible(&self, row: &impl Rowable, mode: FilterType) -> bool {
        let Some(filter) = self.filters.get(&mode) else {
            return true;
        };
        filter.is_row_visible(row)
    }
}

impl Display for Filters {
    /// Implements the `fmt` function for the `Display` or `Debug` trait, allowing the object to be
    /// formatted as a string.
    ///
    /// This function iterates over the `filters` field of the object, where `filters` is expected
    /// to be a collection of key-value pairs (e.g., `HashMap` or similar). For each `(filter, filter_data)`
    /// pair, it converts `filter_data` into a string representation using its `to_string` method.
    /// During this conversion, the method checks if the filter is active by invoking the
    /// `is_filter_active` function on the object, passing `filter` as an argument.
    ///
    /// The string representations of all `filter_data` elements are combined into a single string,
    /// joined by the `" | "` delimiter, and then written to the formatter `f`.
    ///
    /// # Parameters
    /// - `f`: A mutable reference to the `Formatter` instance. This is where the formatted string
    ///   will be written.
    ///
    /// # Returns
    /// - `std::fmt::Result`: Indicates whether the formatting was successful or encountered an error.
    ///
    /// # Expected Struct Fields
    /// - `self.filters`: Should be a collection (e.g., `HashMap`) of filters mapped to their corresponding data.
    /// - `self.is_filter_active(filter: Filter) -> bool`: A method that determines whether a given filter is active.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = vec![];
        for (filter, filter_data) in &self.filters {
            result.push(filter_data.to_string(self.is_filter_active(*filter)));
        }
        write!(f, "{}", result.join("|"))
    }
}

/// Represents a type of filter that can be applied in a system.
///
/// # Variants
/// - `Tenant`: Refers to a filter type specific to tenants.
/// - `Host`: Refers to a filter type specific to hosts.
///
/// # Derives
/// - `Hash`: Allows `FilterType` to be used in hash-based collections (e.g., `HashMap` or `HashSet`).
/// - `PartialEq`: Enables equality comparisons between `FilterType` variants.
/// - `Eq`: Ensures full equivalence comparisons for `FilterType`.
/// - `PartialOrd`: Allows partial ordering between `FilterType` variants.
/// - `Ord`: Ensures total ordering between `FilterType` variants.
/// - `Debug`: Provides a formatted string representation for debugging purposes.
/// - `Copy`: Enables bitwise copying of `FilterType` values.
/// - `Clone`: Allows manual copying of `FilterType` values.
///
/// This enumeration is lightweight and designed for efficient comparisons and usage in ordered or hashed collections.
#[derive(Hash, PartialEq, Eq, Ord, PartialOrd, Debug, Copy, Clone)]
pub enum FilterType {
    Tenant,
    Host,
}

/// A structure representing a filter with a predicate and a name.
///
/// This structure is used to define a filter consisting of a predicate and its associated name.
/// The predicate is typically used as a condition or rule to filter data, while the name provides
/// a human-readable context for the filter.
///
/// # Fields
///
/// - `predicate` (`String`):
///   The condition or rule represented as a string. This is typically used to evaluate or filter data.
///
/// - `name` (`String`):
///   The name associated with the filter. This provides a human-readable identifier or description for the filter.
#[derive(Debug)]
struct Filter {
    predicate: String,
    name: String,
}

impl Filter {
    /// Adds a character to the end of the `predicate` string.
    ///
    /// # Parameters
    /// - `c`: A `char` value representing the character to be appended.
    ///
    /// # Behavior
    /// The method appends the provided `char` (`c`) to the `predicate` string, modifying it in-place.
    ///
    /// # Notes
    /// - Ensure that the `predicate` field is properly initialized before calling this method.
    pub fn push(&mut self, c: char) {
        self.predicate.push(c);
    }

    /// Removes the last element from the `predicate` vector within the structure.
    ///
    /// This method directly calls the `pop` method on the `predicate` vector, which
    /// removes and discards the last element, if any, from the vector.
    ///
    /// # Behavior
    /// - If the `predicate` vector contains elements, the last element is removed.
    /// - If the `predicate` vector is empty, this method does nothing (no error will occur).
    ///
    /// # Notes
    /// - This method does not return the removed element. If you need to retrieve it, you should call
    ///   the `pop` method directly from outside if `predicate` is publicly accessible.
    pub fn pop(&mut self) {
        self.predicate.pop();
    }
}

impl Filter {
    /// Converts the object into a formatted string representation.
    ///
    /// # Parameters
    /// - `active`: A boolean flag indicating whether the object is in an active state.
    ///
    /// # Returns
    /// A `String` that represents the object in a specific format:
    /// - Includes the `self.name` value, followed by the string ` [typing]` if `active` is `true`.
    /// - Includes the value of `self.predicate` if it is not empty; otherwise, defaults to `"<none>"`.
    ///
    /// The resulting string will be formatted as:
    /// ```text
    /// <name>[ [typing]]: <value>
    /// ```
    /// where `<name>` is the value of `self.name` and `<value>` is the value of `self.predicate` or `"<none>"` if `self.predicate` is empty.
    ///
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
    /// Determines if a row is visible based on a given predicate.
    ///
    /// # Parameters
    /// - `row`: A reference to an object that implements the `Rowable` trait.
    ///   This object provides filterable data that can be analyzed for visibility.
    ///
    /// # Returns
    /// - `true` if the `row` contains the specified predicate in its filterable data.
    /// - `false` otherwise.
    ///
    /// # Behavior
    /// This method utilizes the `get_filterable_data` method from the `Rowable` trait,
    /// which provides a collection of data to be filtered. It checks if the given predicate
    /// (embedded in the owning object) exists within the row's filterable data.
    fn is_row_visible(&self, row: &impl Rowable) -> bool {
        row.get_filterable_data().contains(&self.predicate)
    }
}
