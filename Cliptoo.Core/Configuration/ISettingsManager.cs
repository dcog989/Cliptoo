namespace Cliptoo.Core.Configuration
{
    /// <summary>
    /// Manages loading and saving of application settings.
    /// </summary>
    public interface ISettingsManager
    {
        /// <summary>
        /// Loads settings from a persistent source. If no settings are found, returns default settings.
        /// </summary>
        /// <returns>The loaded or default settings.</returns>
        Settings Load();

        /// <summary>
        /// Saves the provided settings to a persistent source.
        /// </summary>
        /// <param name="settings">The settings object to save.</param>
        void Save(Settings settings);
    }
}