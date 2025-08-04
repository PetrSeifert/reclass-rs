# Memory Structure Module

This module provides a modular and extensible memory representation system for the reclass application.

## Overview

The memory structure is designed to represent complex memory layouts with class definitions, instances, and various field types. It supports:

1. **Class Definitions**: Reusable templates for memory structures
2. **Class Instances**: Actual memory instances based on class definitions
3. **Field Types**: Various data types including hex, integers, floats, vectors, text, and class instances
4. **Registry System**: Central storage for class definitions that can be reused

## Architecture

### Core Components

#### `FieldType` (`types.rs`)
Represents all possible field types in the memory structure:
- **Hex types**: Hex64, Hex32, Hex16, Hex8 (no names)
- **Integer types**: Signed and unsigned integers of various sizes
- **Floating point**: Float, Double
- **Vectors**: Vector2, Vector3, Vector4
- **Text**: Text, TextPointer
- **Class instances**: Dynamic size structures

#### `ClassDefinition` (`definitions.rs`)
Represents a reusable class template:
- Contains a list of field definitions
- Calculates total size automatically
- Supports both named and unnamed (hex) fields
- Can reference other class definitions for nested structures

#### `MemoryField` (`nodes.rs`)
Represents an actual field in memory:
- Contains address, data, and error information
- Links to a field type
- Supports editing state

#### `ClassInstance` (`nodes.rs`)
Represents an actual class instance in memory:
- Based on a class definition
- Contains memory fields with actual addresses
- Calculates dynamic sizes

#### `MemoryStructure` (`nodes.rs`)
The root container for the entire memory structure:
- Contains the root class instance
- Manages the class definition registry
- Provides utilities for creating and managing instances

## Usage Example

```rust
use crate::memory::*;

// Create class definitions
let mut player_class = ClassDefinition::new("Player".to_string());
player_class.add_named_field("health".to_string(), FieldType::Int32);
player_class.add_named_field("position".to_string(), FieldType::Vector3);

// Create the memory structure
let memory = MemoryStructureBuilder::new()
    .register_class(player_class)
    .build("GameMemory".to_string(), 0x1000, "Player")
    .expect("Failed to build memory structure");

// Access fields
if let Some(health_field) = memory.root_class.get_field_by_name("health") {
    println!("Health field address: 0x{:X}", health_field.address);
}
```

## Key Features

### Modular Design
- Each component is self-contained and can be extended independently
- Clear separation between definitions and instances
- Registry system allows for reuse of class definitions

### Type Safety
- Strong typing for all field types
- Compile-time checking of field sizes
- Clear distinction between hex fields (no names) and named fields

### Extensibility
- Easy to add new field types
- Support for dynamic sizing (class instances)
- Builder pattern for complex structure creation

### Memory Efficiency
- Class definitions are shared between instances
- Minimal memory overhead for field metadata
- Efficient field lookup by name or index

## Field Type System

### Fixed Size Types
- **Hex types**: Raw memory representation (8, 4, 2, 1 bytes)
- **Integers**: Signed and unsigned variants (8, 4, 2, 1 bytes)
- **Floats**: Single and double precision (4, 8 bytes)
- **Vectors**: 2D, 3D, 4D vectors (8, 12, 16 bytes)
- **Text**: Fixed-size text buffers (32 bytes)
- **TextPointer**: Pointer to text (8 bytes)

### Dynamic Size Types
- **ClassInstance**: Size depends on the referenced class definition

## Class Definition Registry

The registry system allows for:
- **Reuse**: Multiple instances can use the same class definition
- **Organization**: Central management of all class templates
- **Lookup**: Fast access to class definitions by name
- **Validation**: Ensures class definitions exist before creating instances

## Future Extensions

The modular design allows for easy addition of:
- **New field types**: Simply add to the `FieldType` enum
- **Serialization**: Save/load memory structures
- **Validation**: Type checking and size validation
- **Optimization**: Memory layout optimization
- **GUI integration**: Direct integration with the existing GUI system 