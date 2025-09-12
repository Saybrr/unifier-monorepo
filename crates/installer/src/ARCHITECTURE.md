# Installer Library Architecture Map

## Overview

The Installer library is a Rust-based file downloading and validation system designed for installer applications. It provides two main functional areas:
1. **File Downloading** - Multi-protocol downloading with validation and retry capabilities
2. **Wabbajack Integration** - Parsing and converting Wabbajack modlists into download operations

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    installer (lib.rs)                      │
│  Entry Point - Re-exports key types and provides API docs  │
└─────────────────────────┬───────────────────────────────────┘
                          │
          ┌───────────────┴───────────────┐
          │                               │
┌─────────▼──────────┐            ┌──────▼──────────┐
│    downloader      │            │ parse_wabbajack │
│  File downloading  │            │ Modlist parsing │
│   with validation  │            │  and conversion │
└────────────────────┘            └─────────────────┘
```

## NEW Simplified Downloader Architecture

The downloader now uses a trait-based architecture where each source handles its own download logic:

```
┌─────────────────────────────────────────────────────────────┐
│                      User Code                              │
└─────────────────────────┬───────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│              EnhancedDownloader (lib.rs)                   │
│  - Main user interface                                      │
│  - Batch download orchestration with concurrency           │
│  - Metrics collection                                       │
└─────────────────────────┬───────────────────────────────────┘
                          │
                ┌─────────▼─────────┐
                │ DownloadRequest   │
                │ with trait object │
                │ Box<dyn Download> │
                └─────────┬─────────┘
                          │
          ┌───────────────┼───────────────┐
          │               │               │
┌─────────▼──────┐ ┌──────▼──────┐ ┌──────▼──────────┐
│   HttpSource   │ │WabbajackCDN │ │ GameFileSource  │
│impl Download   │ │Source impl  │ │impl Download    │
│- HTTP/HTTPS    │ │Download     │ │- Local copy     │
│- Resume        │ │- Chunked DL │ │- Game discovery │
│- Mirrors       │ │- CDN optim  │ │                 │
└────────────────┘ └─────────────┘ └─────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│                    Core Types                               │
│  - Downloadable trait                                       │
│  - DownloadRequest/Result                                   │
│  - FileValidation (CRC32, MD5, SHA256)                      │
│  - ProgressEvent/Callback                                   │
│  - Error handling                                           │
└─────────────────────────────────────────────────────────────┘
```

## 🎯 **Key Architectural Changes & Benefits**

### **Before: Registry-Based Architecture**
- Central `DownloaderRegistry` routed downloads to appropriate backends
- Separate backend files (`http.rs`, `wabbajack_cdn.rs`, `gamefile.rs`)
- Complex batch orchestration with registry lookups
- Concrete `DownloadSource` enum required pattern matching

### **After: Trait-Based Architecture**
- **Self-Contained Sources**: Each source type implements `Downloadable` trait
- **Eliminated Registry**: No central routing needed - sources know how to download themselves
- **Simplified API**: Just iterate through parsed operations and call `.download()` on each
- **Better Encapsulation**: Download logic lives with the data it operates on

### **Major Benefits**

1. **🔧 Simpler Codebase**
   - Removed ~1,500 lines of registry and backend code
   - Clear separation: each source handles its own protocol
   - No more complex routing logic

2. **🏗️ More Idiomatic Rust**
   - Uses trait system naturally with polymorphism
   - Type-safe at compile time where possible
   - Runtime dispatch only where needed (trait objects)

3. **⚡ Better Performance**
   - Eliminates registry lookup overhead
   - Direct method calls on sources
   - No intermediate conversions

4. **🧪 Easier Testing**
   - Mock individual source types easily
   - Test each source in isolation
   - Clearer test structure

5. **🔄 More Extensible**
   - Add new source types by just implementing `Downloadable`
   - No need to modify registry or routing logic
   - Plugin-friendly architecture

### **Data Flow Comparison**

**Old Flow:**
```
User Code → EnhancedDownloader → batch:: → Registry → Backend → Download
```

**New Flow:**
```
User Code → EnhancedDownloader → Source.download() → Result
```

### Core Components

#### 1. **Core Types (`core/`)**
Foundation types used throughout the system:
- **`mod.rs`** - `DownloadRequest`, `DownloadResult`, main data structures
- **`error.rs`** - `DownloadError`, comprehensive error handling
- **`validation.rs`** - `FileValidation`, hash verification, validation pool
- **`progress.rs`** - `ProgressEvent`, progress reporting system

#### 2. **Configuration (`config.rs`)**
- **`DownloadConfig`** - Download behavior configuration
- **`DownloadConfigBuilder`** - Fluent configuration builder

#### 3. **Registry (`registry.rs`)**
- **`FileDownloader`** trait - Common interface for all backends
- **`DownloaderRegistry`** - Backend selection and management

#### 4. **Batch Operations (`batch/`)**
- **`mod.rs`** - Concurrent download orchestration, retry logic
- **`metrics.rs`** - `DownloadMetrics`, performance tracking

#### 5. **Backend Implementations (`backends/`)**
- **`http.rs`** - `HttpDownloader` for HTTP/HTTPS with resume support
- **`wabbajack_cdn.rs`** - `WabbajackCDNDownloader` for optimized CDN downloads
- **`gamefile.rs`** - `GameFileDownloader` for local game file copying

#### 6. **Main Interface (`lib.rs`)**
- **`EnhancedDownloader`** - Primary user-facing API

## Parse Wabbajack Module Architecture

Handles parsing Wabbajack modlist JSON files and converting them to download operations:

```
┌─────────────────────────────────────────────────────────────┐
│                 Wabbajack JSON File                         │
└─────────────────────────┬───────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│                ModlistParser (parser.rs)                   │
│  - JSON deserialization                                     │
│  - Modlist validation                                       │
│  - Metadata extraction                                      │
└─────────────────────────┬───────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│            Structured Sources (sources.rs)                 │
│  ┌─────────────┐ ┌──────────────┐ ┌─────────────────────┐  │
│  │ HttpSource  │ │ NexusSource  │ │ GameFileSource      │  │
│  │- URL        │ │- Mod ID      │ │- Game path          │  │
│  │- Mirrors    │ │- File ID     │ │- Hash               │  │
│  └─────────────┘ └──────────────┘ └─────────────────────┘  │
│  ┌─────────────┐ ┌──────────────┐                          │
│  │ManualSource │ │ArchiveSource │                          │
│  │- Prompt     │ │- Archive ref │                          │
│  │- URL hint   │ │- State       │                          │
│  └─────────────┘ └──────────────┘                          │
└─────────────────────────┬───────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│           Download Operations (operations.rs)              │
│  - DownloadOperation (combines source + metadata)          │
│  - ArchiveManifest (collection of operations)              │
│  - Operation/Manifest metadata                              │
└─────────────────────────┬───────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│             Integration Layer (integration.rs)             │
│  - operation_to_download_request()                          │
│  - operations_to_download_requests()                        │
│  - Priority-based conversion                                │
│  - Statistics tracking                                      │
└─────────────────────────┬───────────────────────────────────┘
                          │
┌─────────────────────────▼───────────────────────────────────┐
│              DownloadRequest (Core Types)                  │
│  Ready for processing by downloader module                 │
└─────────────────────────────────────────────────────────────┘
```

### Parse Wabbajack Components

#### 1. **Sources (`sources.rs`)**
Structured download source types:
- **`DownloadSource`** - Enum of all source types
- **`HttpSource`** - HTTP URLs with mirror support
- **`NexusSource`** - Nexus Mods integration
- **`GameFileSource`** - Files from game installations
- **`ManualSource`** - Manual download prompts
- **`ArchiveSource`** - References to archives

#### 2. **Operations (`operations.rs`)**
Download operations with metadata:
- **`DownloadOperation`** - Source + destination + metadata
- **`ArchiveManifest`** - Collection of operations for a modlist
- **`OperationMetadata`** - File metadata (size, hash, etc.)

#### 3. **Parser (`parser.rs`)**
- **`ModlistParser`** - JSON parsing and validation
- **`parse_modlist()`** - Main parsing function

#### 4. **Integration (`integration.rs`)**
Conversion from parsed operations to downloader requests:
- **`operation_to_download_request()`** - Single operation conversion
- **`manifest_to_download_requests()`** - Batch conversion
- **`ConversionStats`** - Conversion statistics

## Data Flow

### 1. **Single File Download Flow**
```
User Code
  ↓ DownloadRequest
EnhancedDownloader::download()
  ↓
batch::download_with_retry()
  ↓ Registry lookup by source type
DownloaderRegistry
  ↓ Route to appropriate backend
HttpDownloader/WabbajackCDNDownloader/GameFileDownloader
  ↓ Execute download with validation
DownloadResult
```

### 2. **Wabbajack Processing Flow**
```
Wabbajack JSON
  ↓ parse_modlist()
ModlistParser
  ↓ ArchiveManifest
Integration Layer
  ↓ manifest_to_download_requests()
Vec<DownloadRequest>
  ↓
EnhancedDownloader::download_batch()
  ↓
Vec<DownloadResult>
```

### 3. **Batch Download Flow**
```
Vec<DownloadRequest>
  ↓
EnhancedDownloader::download_batch_with_async_validation()
  ↓ Concurrent processing with semaphore
batch::download_batch_with_async_validation()
  ↓ Per-request processing
batch::download_with_async_validation()
  ↓ Background validation
ValidationPool
  ↓
Vec<DownloadResult>
```

## Key Design Principles

### 1. **Structured Types Over Strings**
- Uses strongly-typed `DownloadSource` enum instead of URL strings
- Enables better type safety and richer metadata
- Allows protocol-specific optimizations

### 2. **Layered Architecture**
- Clear separation between user interface, orchestration, and implementation
- Each layer has well-defined responsibilities
- Easy to extend with new backends or features

### 3. **Async-First Design**
- Built on Tokio async runtime
- Concurrent downloads with configurable limits
- Background validation for improved performance

### 4. **Comprehensive Error Handling**
- Rich error types with context information
- Retry logic with exponential backoff
- Mirror URL fallback support

### 5. **Progress and Metrics**
- Real-time progress reporting
- Built-in performance metrics
- Composable progress reporters

### 6. **Extensible Backend System**
- `FileDownloader` trait for easy backend addition
- Registry-based backend selection
- Protocol-agnostic request/response types

## File Organization

```
src/
├── lib.rs                    # Main entry point, API documentation
├── downloader.rs             # Downloader module root, re-exports
├── downloader/
│   ├── lib.rs               # EnhancedDownloader main interface
│   ├── config.rs            # Configuration types and builder
│   ├── registry.rs          # Backend registry and trait
│   ├── core/
│   │   ├── mod.rs          # Core request/response types
│   │   ├── error.rs        # Error handling
│   │   ├── validation.rs   # File validation system
│   │   └── progress.rs     # Progress reporting
│   ├── batch/
│   │   ├── mod.rs          # Batch download orchestration
│   │   └── metrics.rs      # Performance metrics
│   ├── backends/
│   │   ├── mod.rs          # Backend module root
│   │   ├── http.rs         # HTTP downloader
│   │   ├── wabbajack_cdn.rs # WabbajackCDN downloader
│   │   └── gamefile.rs     # GameFile downloader
│   └── tests.rs            # Integration tests
└── parse_wabbajack/
    ├── mod.rs              # Module root, re-exports
    ├── sources.rs          # Download source types
    ├── operations.rs       # Operation and manifest types
    ├── parser.rs           # JSON parsing logic
    └── integration.rs      # Conversion to download requests
```

This architecture provides a clean, extensible foundation for file downloading and installer functionality, with clear separation of concerns and strong type safety throughout.

