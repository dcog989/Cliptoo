# Coding Rules

## General

- Follow existing code patterns and conventions in the project.
- Use `nullable` reference types properly - all nullable references must be marked with `?`.
- Use `var` for local variable declarations when type is obvious.
- Use expression-bodied members for simple properties and methods.
- Prefer `async`/`await` over `.Result` or `.Wait()`.

## Naming Conventions

- **PascalCase**: Classes, methods, properties, public fields
- **_camelCase**: Private fields with underscore prefix
- **camelCase**: Parameters, local variables
- **I{Name}**: Interfaces

## WPF/MVVM

- ViewModels inherit from `ViewModelBase`
- Use `ObservableCollection<T>` for collections bound to UI
- Use `INotifyPropertyChanged` via `SetProperty` method in base class
- Keep code-behind minimal; logic goes in ViewModels

## Database

- Use Dapper-style parameterized queries
- All database operations go through `ClipRepository` or `RepositoryBase`
- Use FTS5 for text search queries

## Error Handling

- Use specific exception types in `catch` blocks
- Log errors via `LogManager.LogError` / `LogManager.LogCritical`
- Never swallow exceptions silently

## Performance

- Use `ConfigureAwait(false)` in library code (Core project)
- Avoid UI thread blocking - use `async` methods
- Cache expensive computations (thumbnails, metadata)
- Use virtualization for large lists
