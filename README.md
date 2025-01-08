# ReClass-RS

A Rust implementation of ReClass, providing advanced memory analysis and reverse engineering capabilities with a modern GUI built with egui.

## 🎯 Overview

ReClass-RS is a memory analysis tool designed for reverse engineering. It provides an intuitive interface for exploring and analyzing memory structures in running processes, making it easier to understand complex data layouts and memory patterns.

## ✨ Features

### 🔍 Process Analysis
- **Process Attaching**: Attach to running processes and analyze their memory
- **Module Listing**: View and explore loaded modules within processes
- **Memory Reading**: Read memory at specific addresses

### 🏗️ Memory Structure Analysis
- **Class Elements**: Create hierarchical memory structures with classes and fields
- **Field Types**: Support for various data types:
  - **Hex Types**: Hex64, Hex32, Hex16, Hex8
  - **Integers**: Signed (Int64, Int32, Int16, Int8) and Unsigned (UInt64, UInt32, UInt16, UInt8)
  - **Floating Point**: Float, Double
  - **Vectors**: Vector4, Vector3, Vector2
  - **Text**: Text, TextPointer
  - **Boolean**: Bool

## 🚀 Installation

### Prerequisites
- Rust 1.70 or later
- Driver

### Building from Source

1. **Clone the repository**:
   ```bash
   git clone https://github.com/PetrSeifert/reclass-rs.git
   cd reclass-rs
   ```

2. **Build the project**:
   ```bash
   cargo build --release
   ```

3. **Run the application**:
   ```bash
   cargo run --release
   ```

## 📖 Usage

### Getting Started

1. **Launch the Application**: Run the compiled binary or use `cargo run --release`

2. **Attach to a Process**:
   - Click the "Attach to Process" button in the top menu
   - Select a target process from the list
   - The application will create a handle to the process

3. **Explore Memory**:
   - Use the memory address input to read specific addresses
   - Right-click elements to add new memory structures
   - Double-click names to edit them inline

### Memory Structure Management

#### Creating Classes
- Right-click on any element and select "New Class"
- Classes act as containers for related fields

#### Adding Fields
- Right-click on a class or field and select "Add Element After"
- Choose the field type from the type selection window

## 🏗️ Project Structure

```
reclass-rs/
├── main/                    # Main application
│   ├── src/
│   │   ├── gui/            # GUI components
│   │   │   ├── gui.rs      # Main GUI logic
│   │   │   ├── memory_elements.rs  # Memory element definitions
│   │   │   ├── selection.rs # Multi-selection management
│   │   │   ├── windows.rs   # Window management
│   │   │   └── rendering.rs # UI rendering logic
│   │   └── main.rs         # Application entry point
├── handle/                  # Process handle management
│   └── src/
│       ├── handle.rs       # Process handle implementation
│       ├── pattern.rs      # Pattern matching
│       └── signature.rs    # Signature scanning
```

## 🔧 Development

### Prerequisites
- Rust toolchain (latest nightly)

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- **ReClass.NET**: Original inspiration and reference implementation
- **egui**: Modern, fast GUI framework for Rust

## 🔮 Roadmap

- [ ] Pattern scanning and signature matching
- [ ] Export/import of memory structures

---

**Note**: This tool is intended for legitimate reverse engineering, security research, and educational purposes. Please ensure you have proper authorization before analyzing any processes or memory. 