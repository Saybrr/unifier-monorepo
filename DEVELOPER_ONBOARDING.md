# *AI Generated* Developer Onboarding Guide
## Technology Stack

### Backend
- **Rust** - Core backend language
- **Tauri** - Desktop application framework
- **SurrealDB** - Embedded database (in-memory for dev)
- **Tokio** - Async runtime

### Frontend
- **TypeScript** - Frontend language
- **Native Web Components** - Custom elements without frameworks
- **DOM-Native** - Lightweight utility library for web components
- **PostCSS** - CSS processing and hot reload

### Build Tools
- **Awesome App CLI** - Development orchestration (`awesome-app dev`)
- **Rollup** - TypeScript bundling
- **PostCSS CLI** - CSS processing

## Architecture Overview: VMES Pattern

The application follows the **VMES (View-Model-Event-Store)** architecture pattern:

```
┌─────────────────────────────────────┐
│              FRONTEND               │
│  ┌─────────┐ ┌───────┐ ┌─────────┐  │
│  │  View   │ │ Model │ │  Event  │  │
│  │ Layer   │ │ Layer │ │ Layer   │  │
│  └─────────┘ └───────┘ └─────────┘  │
└─────────────────────────────────────┘
               │ IPC │
┌─────────────────────────────────────┐
│              BACKEND                │
│  ┌─────────┐ ┌───────┐ ┌─────────┐  │
│  │  Model  │ │ Event │ │  Store  │  │
│  │  Layer  │ │ Layer │ │ Layer   │  │
│  └─────────┘ └───────┘ └─────────┘  │
└─────────────────────────────────────┘
```

### Key Principle: Event-Driven Architecture
- UI sends commands and "forgets" about them
- Backend fires model events that propagate back to frontend
- Frontend updates UI based on received events
- This prevents state management nightmares and scales well

## Project Structure

### Backend (`src-tauri/src/`)
```
src-tauri/src/
├── main.rs                 # Application entry point
├── ctx.rs                  # Context management
├── error.rs               # Error handling
├── event.rs               # Event definitions
├── prelude.rs             # Common imports
├── ipc/                   # IPC layer - bridges frontend to backend
│   ├── mod.rs             # Re-exports
│   ├── params.rs          # IPC parameter types
│   ├── project.rs         # Project IPC handlers
│   └── task.rs            # Task IPC handlers
├── model/                 # Model layer - business logic
│   ├── mod.rs             # Model exports and types
│   ├── project.rs         # Project model and BMC
│   ├── task.rs            # Task model and BMC
│   ├── bmc_base.rs        # Base Model Controller functions
│   └── store/             # Data persistence layer
└── utils/                 # Utility functions
```

### Frontend (`src-ui/src/`)
```
src-ui/src/
├── main.ts                # Frontend entry point
├── ipc.ts                 # IPC communication utilities
├── event.ts               # Event handling
├── router.ts              # Client-side routing
├── bindings/              # Auto-generated TypeScript types from Rust
├── model/                 # Frontend Model Controllers (FMC)
│   └── index.ts           # Project and Task FMCs
└── view/                  # View layer - web components
    ├── app-v.ts           # Main application view
    ├── nav-v.ts           # Navigation view
    ├── project-v.ts       # Project view
    └── tasks-dt.ts        # Tasks data table
```

## Core Patterns and Conventions

### 1. Backend Model Pattern (BMC)

Every entity follows the **5-construct pattern** in `src-tauri/src/model/`:

**Example: `project.rs`**
1. **Entity Type** (`Project`) - What gets sent to frontend
2. **Create Type** (`ProjectForCreate`) - Data for creating new entities
3. **Update Type** (`ProjectForUpdate`) - Data for updating entities
4. **Filter Type** (`ProjectFilter`) - Query filtering
5. **Backend Model Controller** (`ProjectBmc`) - CRUD operations

```rust
// 1. Entity Type
#[derive(Serialize, TS, Debug)]
#[ts(export, export_to = "../src-ui/src/bindings/")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub ctime: String,
}

// 2. Create Type
#[derive(Deserialize, TS, Debug)]
pub struct ProjectForCreate {
    pub name: String,
}

// 3. Update Type
#[derive(Deserialize, TS, Debug)]
pub struct ProjectForUpdate {
    pub name: Option<String>,
}

// 4. Filter Type
#[derive(FilterNodes, Deserialize, Debug)]
pub struct ProjectFilter {
    pub name: Option<OpValsString>,
}

// 5. Backend Model Controller
pub struct ProjectBmc;
impl ProjectBmc {
    const ENTITY: &'static str = "project";

    pub async fn get(ctx: Arc<Ctx>, id: &str) -> Result<Project> { ... }
    pub async fn create(ctx: Arc<Ctx>, data: ProjectForCreate) -> Result<ModelMutateResultData> { ... }
    pub async fn update(ctx: Arc<Ctx>, id: &str, data: ProjectForUpdate) -> Result<ModelMutateResultData> { ... }
    pub async fn delete(ctx: Arc<Ctx>, id: &str) -> Result<ModelMutateResultData> { ... }
    pub async fn list(ctx: Arc<Ctx>, filter: Option<ProjectFilter>) -> Result<Vec<Project>> { ... }
}
```

### 2. Type Safety with `ts-rs`

Rust types are the **source of truth**. TypeScript bindings are auto-generated:

```rust
#[derive(Serialize, TS, Debug)]
#[ts(export, export_to = "../src-ui/src/bindings/")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub ctime: String,
}
```

This generates `src-ui/src/bindings/Project.ts` with matching TypeScript types.

### 3. Store Layer Abstraction

The store layer (`src-tauri/src/model/store/`) doesn't know about specific entities, only about generic traits:

```rust
pub trait Creatable: Into<Value> {}
pub trait Patchable: Into<Value> {}
pub trait Filterable: IntoFilterNodes {}
```

This allows the same store implementation to work with any entity type.

### 4. Frontend Model Controllers (FMC)

Frontend mirrors backend with **Frontend Model Controllers**:

```typescript
// src-ui/src/model/index.ts
class BaseFmc<M, C, U> {
  async get(id: string): Promise<M> { ... }
  async create(data: C): Promise<ModelMutateResultData> { ... }
  async update(id: string, data: U): Promise<ModelMutateResultData> { ... }
  async delete(id: string): Promise<ModelMutateResultData> { ... }
}

class ProjectFmc extends BaseFmc<Project, ProjectForCreate, ProjectForUpdate> {
  async list(): Promise<Project[]> { ... }
}

export const projectFmc = new ProjectFmc();
```

### 5. Native Web Components

Views use native web components with `dom-native` utilities:

```typescript
// src-ui/src/view/app-v.ts
@customElement('app-v')
export class AppView extends BaseHTMLElement {
  // Key elements cached for performance
  #mainEl!: HTMLElement

  // App-level events (non-DOM)
  @onHub("Route", "change")
  async onRouteChange() { ... }

  // DOM events
  @onEvent("pointerup", "header > d-ico.menu")
  onMenuClick(evt: PointerEvent) { ... }

  init() {
    // Initialize component
    let content = document.importNode(HTML, true);
    this.#mainEl = getFirst(content, "main");
    this.replaceChildren(content);
  }
}
```

### 6. Event-Driven Updates

Components listen for model events and update accordingly:

```typescript
// src-ui/src/view/tasks-dt.ts
@onHub("Model", "task", "create")
onTaskCreate() {
    this.update(); // Refresh full table
}

@onHub("Model", "task", "update")
async onTaskUpdate(data: ModelMutateResultData) {
    const newTask = await taskFmc.get(data.id);
    // Update specific row
    const taskEl = this.querySelector(`task-row.${classable(data.id)}`);
    taskEl.task = newTask;
}
```

## Development Workflow

### 1. Hot Reload Development

Start the development environment:
```bash
awesome-app dev
```

This runs all necessary processes concurrently (defined in `Awesome.toml`):
- `npm install` - Dependencies
- `cargo build` - Rust compilation
- `pcss -w` - CSS hot reload
- `rollup -w` - TypeScript hot reload
- `localhost` server - Frontend serving
- `tauri dev` - Desktop app with hot reload

### 2. Adding New Entities

To add a new entity (e.g., `Comment`):

#### Backend:
1. Create `src-tauri/src/model/comment.rs` following the 5-construct pattern
2. Add IPC handlers in `src-tauri/src/ipc/comment.rs`
3. Register handlers in `src-tauri/src/main.rs`

#### Frontend:
1. Auto-generated TypeScript types appear in `src-ui/src/bindings/`
2. Create FMC in `src-ui/src/model/index.ts`
3. Create view components in `src-ui/src/view/`

### 3. CSS Organization

CSS follows component naming conventions:
- `src-ui/pcss/view/app-v.pcss` - Styles for `app-v` component
- `src-ui/pcss/view/tasks-dt.pcss` - Styles for `tasks-dt` component

Naming suffixes:
- `-v` - Main view components
- `-c` - Smaller components
- `-dt` - Data table components

### 4. Debugging

The desktop app includes full Chrome DevTools:
- Right-click → "Inspect Element"
- Full DOM inspection, console, network tabs
- Debug TypeScript with source maps

## Key Files Reference

### Essential Backend Files
- `src-tauri/src/main.rs` - App initialization, IPC handler registration
- `src-tauri/src/model/project.rs` - Example of complete entity implementation
- `src-tauri/src/model/store/mod.rs` - Store layer traits and abstractions
- `src-tauri/src/ipc/project.rs` - IPC bridge between frontend and backend
- `src-tauri/src/ctx.rs` - Context management for database access

### Essential Frontend Files
- `src-ui/src/main.ts` - Frontend initialization
- `src-ui/src/model/index.ts` - Frontend Model Controllers (FMCs)
- `src-ui/src/view/app-v.ts` - Main application component
- `src-ui/src/view/tasks-dt.ts` - Complex data table component example
- `src-ui/src/bindings/` - Auto-generated TypeScript types

### Configuration Files
- `Awesome.toml` - Development orchestration configuration
- `package.json` - Node.js dependencies and scripts
- `src-tauri/Cargo.toml` - Rust dependencies
- `src-tauri/Tauri.toml` - Tauri application configuration

## Common Development Tasks

### Modifying Data Models
To add a field to `Project`:

1. **Backend**: Update `src-tauri/src/model/project.rs`
   ```rust
   pub struct Project {
       pub id: String,
       pub name: String,
       pub description: Option<String>, // New field
       pub ctime: String,
   }
   ```

2. **Auto-generated types** will update in `src-ui/src/bindings/Project.ts`

3. **Frontend** components automatically get new TypeScript types

### Adding UI Components
1. Create new file in `src-ui/src/view/` (e.g., `comment-list.ts`)
2. Follow naming convention: file name matches custom element tag
3. Extend `BaseHTMLElement` and use `@customElement` decorator
4. Create corresponding CSS file in `src-ui/pcss/view/`

### Database Changes
- Development uses in-memory SurrealDB (data resets on restart)
- For persistence, enable full SurrealDB features in `src-tauri/Cargo.toml`
- Database schema is created dynamically by the store layer

## Best Practices

### 1. Event-Driven Updates
- **Don't** update UI immediately after mutations
- **Do** emit events and let components update via event handlers
- This keeps the UI in sync with actual backend state

### 2. Component Responsibility
- Keep individual components "dumb" - they display data and emit events
- Container components handle event coordination and data management
- Example: `TaskRow` just displays; `TasksDataTable` handles all logic

### 3. Type Safety
- Leverage auto-generated TypeScript types from Rust
- Use generic base classes to ensure type consistency
- Let the compiler catch errors early

### 4. Performance
- Cache key DOM elements in component properties
- Use efficient DOM queries with specific selectors
- Leverage native web component lifecycle methods

This architecture scales from simple desktop apps to complex applications with the same patterns, whether targeting desktop, mobile (with Tauri 2.0), or cloud deployments.
