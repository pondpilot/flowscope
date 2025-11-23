# FlowScope App Architecture

## Overview

The FlowScope app is a production-ready web application for SQL lineage visualization. It has been refactored from a demo state to follow professional software engineering practices with clean separation of concerns, proper state management, and maintainable code structure.

## Directory Structure

```
app/src/
├── components/          # React components
│   ├── ui/             # Reusable UI primitives (Radix-based)
│   ├── AnalysisView.tsx    # Lineage visualization panel
│   ├── EditorArea.tsx      # SQL editor panel
│   ├── EditorToolbar.tsx   # Editor controls and settings
│   ├── ErrorBoundary.tsx   # Error boundary wrapper
│   ├── FileExplorer.tsx    # Project file browser
│   └── Workspace.tsx       # Main layout component
├── hooks/              # Custom React hooks
│   ├── useAnalysis.ts      # SQL analysis logic
│   ├── useFileNavigation.ts # Graph-to-file navigation
│   ├── useKeyboardShortcuts.ts # Keyboard shortcut system
│   └── useWasmInit.ts      # WASM initialization
├── lib/                # Core utilities and state
│   ├── constants.ts        # App-wide constants
│   ├── project-store.tsx   # Project state management
│   └── utils.ts            # Utility functions
├── types/              # TypeScript type definitions
│   └── index.ts
├── App.tsx             # Root application component
└── main.tsx            # Application entry point
```

## Key Design Patterns

### 1. Separation of Concerns

**Before:** Large monolithic components with mixed responsibilities
**After:** Clear separation between:
- **UI Components**: Pure presentation logic
- **Hooks**: Business logic and state management
- **Store**: Application state
- **Types**: Type definitions
- **Constants**: Configuration values

### 2. Custom Hooks Pattern

All complex logic has been extracted into reusable hooks:

#### `useAnalysis`
Manages SQL analysis workflow:
- File validation
- Analysis context building
- Error handling
- Integration with WASM engine

#### `useFileNavigation`
Handles navigation from graph view to source files

#### `useKeyboardShortcuts`
Provides keyboard shortcut registration system

#### `useWasmInit`
Manages WASM module initialization with retry logic

### 3. Component Composition

**EditorArea** has been decomposed:
- `EditorArea.tsx` - Container component
- `EditorToolbar.tsx` - Toolbar with controls (extracted)
- Uses hooks for logic (not inline)

### 4. Centralized Configuration

All magic numbers and configuration moved to `lib/constants.ts`:
```typescript
FILE_LIMITS.MAX_SIZE        // 10MB
FILE_LIMITS.MAX_COUNT       // 100 files
KEYBOARD_SHORTCUTS          // Keyboard bindings
STORAGE_KEYS               // LocalStorage keys
UI_CONFIG                  // Layout dimensions
```

### 5. Type Safety

Created `types/index.ts` for app-level types:
- `WasmState` - WASM initialization state
- `AnalysisState` - Analysis workflow state
- `KeyboardShortcutHandler` - Shortcut configuration
- `FileValidationResult` - Validation results
- `AnalysisContext` - Analysis parameters

### 6. Error Handling

Improved error handling system:
- Custom `Toast` component for notifications
- Structured error states in hooks
- Error boundaries for component crashes
- WASM initialization retry logic

## State Management

### Project Store (`lib/project-store.tsx`)

Context-based state management for:
- Multiple projects
- File management (create, update, delete, rename)
- Project configuration (dialect, run mode)
- File selection for analysis
- LocalStorage persistence

**Improvements:**
- Error handling in storage operations
- Helper function for file language detection
- Immutable update patterns
- Proper TypeScript typing

### Lineage State (`@pondpilot/flowscope-react`)

Provided by the library package:
- Analysis results
- Selected nodes/statements
- View mode preferences
- Navigation requests

## Component Responsibilities

### App.tsx
- WASM initialization
- Provider composition
- Dark mode configuration

### Workspace.tsx
- Three-panel layout (sidebar, editor, analysis)
- Header with branding
- Error banner display
- Panel resizing logic

### EditorArea.tsx
- File content editing
- Analysis triggering
- File renaming
- Keyboard shortcuts
- Error display

### EditorToolbar.tsx
- File name display/editing
- Dialect selector
- Run configuration dropdown
- Analysis button with loading state

### FileExplorer.tsx
- Project selection
- File list display
- File creation/import
- Visual indication of included files
- Custom selection mode

### AnalysisView.tsx
- Tabbed interface (Lineage, Schema, Issues)
- Summary statistics
- Integration with library components

## Data Flow

### Analysis Flow

```
User clicks "Run" in EditorToolbar
    ↓
EditorArea.handleAnalyze()
    ↓
useAnalysis.runAnalysis()
    ↓
1. Build analysis context based on run mode
2. Validate files (count, size)
3. Call analyzeSql() from WASM
4. Update lineage state via actions.setResult()
    ↓
AnalysisView receives new result
    ↓
GraphView, IssuesPanel, SchemaView re-render
```

### Navigation Flow

```
User clicks node in GraphView
    ↓
Graph emits navigation request
    ↓
useFileNavigation detects request
    ↓
Finds matching file by name
    ↓
Calls selectFile() to switch active file
    ↓
EditorArea updates with new file content
```

## Performance Optimizations

1. **useCallback** for stable function references
2. **Lazy WASM initialization** with concurrent call handling
3. **Memoized selectors** in project store
4. **Immutable update patterns** to avoid unnecessary re-renders
5. **Code splitting** via dynamic imports (Vite)

## Keyboard Shortcuts

Defined in `KEYBOARD_SHORTCUTS` constant:
- `Cmd/Ctrl + Enter` - Run SQL analysis
- `Cmd/Ctrl + B` - Toggle sidebar (planned)
- `Cmd/Ctrl + N` - New file (planned)
- `Cmd/Ctrl + S` - Save (planned)

## Configuration

### File Limits
- Max file size: 10MB
- Max file count: 100

### UI Layout
- Header height: 48px
- Editor toolbar: 50px
- Default panel sizes: 20% / 40% / 40%

### Supported File Types
- `.sql` - SQL scripts
- `.json` - Schema/config files
- `.txt` - Text files

## Error Handling Strategy

### WASM Errors
- Retry mechanism with loading state
- User-friendly error messages
- Prevents app crash on WASM failure

### Analysis Errors
- File validation before analysis
- Clear error messages with context
- Toast notifications for user feedback
- Non-blocking (doesn't crash editor)

### Storage Errors
- Try/catch around localStorage operations
- Console warnings for debugging
- Fallback to default state

## Testing Recommendations

### Unit Tests (to be added)
- Hook logic (useAnalysis, useWasmInit)
- File validation functions
- Storage helpers
- Utility functions

### Integration Tests (to be added)
- Full analysis workflow
- Project/file management
- Navigation between files
- Keyboard shortcuts

### E2E Tests (to be added)
- Load app and analyze SQL
- Multi-file project analysis
- Error scenarios
- Export functionality

## Future Improvements

### Planned Features
1. **Command Palette** - Quick actions and navigation
2. **File Search** - Find files quickly
3. **Recent Files** - Quick access to recently edited
4. **Syntax Validation** - Pre-analysis SQL checks
5. **Export Project** - Download entire project
6. **Import Project** - Upload zip with multiple files
7. **Undo/Redo** - For file edits
8. **Split Editor** - View multiple files side-by-side

### Code Quality
1. Add comprehensive test suite
2. Extract more granular components
3. Implement proper error boundaries per feature
4. Add performance monitoring
5. Implement analytics (optional)

### UX Enhancements
1. Loading skeletons for better perceived performance
2. Onboarding tour for new users
3. Contextual help tooltips
4. Customizable keyboard shortcuts
5. Theme customization

## Migration Notes

If upgrading from demo version:

1. **Storage Migration**: Projects are automatically migrated with default values for new fields
2. **Component Imports**: Some components removed (ProjectSidebar, ExportMenu from app)
3. **Hook Usage**: Analysis logic now in `useAnalysis` hook
4. **Constants**: Magic numbers replaced with named constants
5. **Types**: New type definitions in `types/index.ts`

## Development Guidelines

### Adding New Features

1. **Create types** in `types/index.ts` if needed
2. **Add constants** to `lib/constants.ts`
3. **Extract logic to hooks** if complex
4. **Keep components focused** on presentation
5. **Write tests** for new functionality

### Code Style

- Use functional components with hooks
- Extract business logic to custom hooks
- Use named exports (not default)
- Prefer `const` over `let`
- Use TypeScript strict mode
- Follow existing patterns for consistency

### State Management

- Use Context for global state (projects, lineage)
- Use local state for UI-only concerns
- Keep state as close to usage as possible
- Avoid prop drilling with context or composition

## Dependencies

### Core
- React 18.2.0
- TypeScript 5.0+
- Vite 5.0 (build tool)

### UI
- Radix UI (primitives)
- Lucide React (icons)
- Tailwind CSS (styling)
- tailwind-merge (class merging)

### FlowScope
- @pondpilot/flowscope-core (WASM engine)
- @pondpilot/flowscope-react (UI components)

### Utilities
- react-resizable-panels (layout)
- class-variance-authority (variants)

## Conclusion

The refactored FlowScope app follows professional React patterns with clear separation of concerns, proper error handling, and maintainable code structure. The architecture supports future growth while maintaining simplicity and performance.
