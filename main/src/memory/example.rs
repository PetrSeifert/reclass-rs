use crate::memory::types::FieldType;
use crate::memory::definitions::ClassDefinition;
use crate::memory::nodes::{MemoryStructure, MemoryStructureBuilder};

/// Example function that creates the memory structure from your specification
pub fn create_example_memory_structure() -> MemoryStructure {
    // Create class definitions
    let mut class_name1 = ClassDefinition::new("class_name1".to_string());
    
    // Add fields to class_name1
    class_name1.add_hex_field(FieldType::Hex64);
    class_name1.add_named_field("var_name1".to_string(), FieldType::Int64);
    
    // Create class_name2 definition
    let mut class_name2 = ClassDefinition::new("class_name2".to_string());
    class_name2.add_hex_field(FieldType::Hex32);
    class_name2.add_named_field("var_name3".to_string(), FieldType::Bool);
    class_name2.add_hex_field(FieldType::Hex8);
    class_name2.add_hex_field(FieldType::Hex16);
    class_name2.add_hex_field(FieldType::Hex32);
    
    // Add class instances to class_name1
    class_name1.add_class_instance("var_name2".to_string(), &class_name2);
    class_name1.add_named_field("var_name4".to_string(), FieldType::TextPointer);
    class_name1.add_class_instance("var_name5".to_string(), &class_name2); // Reused class definition
    
    // Build the memory structure
    let mut builder = MemoryStructureBuilder::new();
    builder.register_class(class_name1.clone());
    builder.register_class(class_name2);
    let mut structure = builder.build("RootClass".to_string(), 0x1000, "class_name1")
        .expect("Failed to build memory structure");
    
    // Create nested instances
    structure.create_nested_instances();
    structure
}

/// Example function that demonstrates how to traverse the memory structure
pub fn traverse_memory_structure(memory: &MemoryStructure) {
    println!("Memory Structure:");
    println!("Root class: {}", memory.root_class.get_display_name());
    println!("Address: 0x{:X}", memory.root_class.address);
    println!("Size: {} bytes", memory.root_class.get_size());
    
    println!("\nFields:");
    for (i, field) in memory.root_class.fields.iter().enumerate() {
        let indent = "  ";
        println!("{}{}. {}", indent, i + 1, field.get_display_name());
        println!("{}   Address: 0x{:X}", indent, field.address);
        println!("{}   Size: {} bytes", indent, field.get_size());
        
        // If this is a class instance, we would need to handle it specially
        if field.field_type == FieldType::ClassInstance {
            println!("{}   Type: ClassInstance (dynamic size)", indent);
        }
    }
    
    println!("\nAvailable class definitions:");
    for class_name in memory.get_available_classes() {
        println!("  - {}", class_name);
    }
}

/// Example function that shows how to create a more complex memory structure
pub fn create_complex_memory_structure() -> MemoryStructure {
    // Create a Player class
    let mut player_class = ClassDefinition::new("Player".to_string());
    player_class.add_named_field("health".to_string(), FieldType::Int32);
    player_class.add_named_field("max_health".to_string(), FieldType::Int32);
    player_class.add_named_field("position".to_string(), FieldType::Vector3);
    player_class.add_named_field("is_alive".to_string(), FieldType::Bool);
    player_class.add_hex_field(FieldType::Hex32); // Padding
    player_class.add_named_field("name".to_string(), FieldType::TextPointer);
    
    // Create a Weapon class
    let mut weapon_class = ClassDefinition::new("Weapon".to_string());
    weapon_class.add_named_field("damage".to_string(), FieldType::Int32);
    weapon_class.add_named_field("ammo".to_string(), FieldType::Int32);
    weapon_class.add_named_field("max_ammo".to_string(), FieldType::Int32);
    weapon_class.add_named_field("is_loaded".to_string(), FieldType::Bool);
    
    // Create a GameState class that contains players and weapons
    let mut game_state_class = ClassDefinition::new("GameState".to_string());
    game_state_class.add_named_field("player_count".to_string(), FieldType::Int32);
    game_state_class.add_class_instance("current_player".to_string(), &player_class);
    game_state_class.add_class_instance("weapon".to_string(), &weapon_class);
    game_state_class.add_hex_field(FieldType::Hex64); // Some padding
    game_state_class.add_named_field("game_time".to_string(), FieldType::Double);
    
    // Build the memory structure
    let mut builder = MemoryStructureBuilder::new();
    builder.register_class(game_state_class.clone());
    builder.register_class(player_class);
    builder.register_class(weapon_class);
    builder.build("GameMemory".to_string(), 0x2000, "GameState")
        .expect("Failed to build memory structure")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_example_memory_structure() {
        let memory = create_example_memory_structure();
        assert_eq!(memory.root_class.name, "RootClass");
        assert_eq!(memory.root_class.class_definition.name, "class_name1");
        assert!(memory.get_available_classes().contains(&"class_name1".to_string()));
        assert!(memory.get_available_classes().contains(&"class_name2".to_string()));
    }

    #[test]
    fn test_complex_memory_structure() {
        let memory = create_complex_memory_structure();
        assert_eq!(memory.root_class.name, "GameMemory");
        assert_eq!(memory.root_class.class_definition.name, "GameState");
        
        // Check that all classes are registered
        let available_classes = memory.get_available_classes();
        assert!(available_classes.contains(&"GameState".to_string()));
        assert!(available_classes.contains(&"Player".to_string()));
        assert!(available_classes.contains(&"Weapon".to_string()));
    }
} 