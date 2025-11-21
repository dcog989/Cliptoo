@{
    # Script module or binary module file associated with this manifest.
    RootModule           = 'BuilderToolbox.psm1'

    # Version number of this module.
    ModuleVersion        = '1.17.0'

    # Supported PSEditions
    CompatiblePSEditions = @('Desktop', 'Core')

    # ID used to uniquely identify this module
    GUID                 = 'a4b3c2d1-e5f6-4a5b-8c9d-0e1f2a3b4c5d'

    # Author of this module
    Author               = 'Builder Toolbox'

    # Company or vendor of this module
    CompanyName          = 'Unknown'

    # Copyright statement for this module
    Copyright            = '(c) 2025 Builder Toolbox. All rights reserved.'

    # Description of the functionality provided by this module
    Description          = 'A comprehensive CLI toolkit for managing .NET application builds, testing, packaging, and deployment.'

    # Functions to export from this module, for best performance, do not use wildcards
    FunctionsToExport    = @(
        'Main',
        'Show-Menu'
    )

    # Cmdlets to export from this module
    CmdletsToExport      = @()

    # Variables to export from this module
    VariablesToExport    = @()

    # Aliases to export from this module
    AliasesToExport      = @()

    # List of all modules packaged with this module
    # Order matters: Core dependencies first, then feature modules.
    NestedModules        = @(
        'Classes.ps1',
        'Configuration.ps1',
        'Logging.ps1',
        'Utilities.ps1',
        'Dotnet.ps1',
        'GitAndTools.ps1',
        'BuildActions.ps1',
        'PackageActions.ps1',
        'PublishActions.ps1',
        'MaintenanceActions.ps1',
        'ExplorerActions.ps1'
    )
}