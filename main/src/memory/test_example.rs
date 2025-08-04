use crate::memory::example::*;

pub fn run_memory_structure_test() {
    println!("=== Memory Structure Test ===");
    
    // Create the example memory structure
    let memory = create_example_memory_structure();
    
    // Traverse and display the structure
    traverse_memory_structure(&memory);
    
    println!("\n=== Complex Memory Structure Test ===");
    
    // Create a more complex example
    let complex_memory = create_complex_memory_structure();
    traverse_memory_structure(&complex_memory);
    
    println!("\n=== Test completed successfully ===");
} 