// Phase 1: Install files from archives (uses VFS)
async fn install_archives(&self, directives: &[FromArchiveDirective]) -> Result<()> {
    for directive in directives.iter().filter(|d| matches!(d, FromArchiveDirective::FromArchive(_))) {
        // Use VFS to locate and extract file
        let vfs_node = self.vfs.find_file(&directive.archive_hash_path)?;
        let output_path = self.install_dir.join(&directive.to);
        // Extract from archive to output_path
    }
}

// Phase 2: Install inline files (bypasses VFS completely)
async fn install_included_files(&self, directives: &[InlineFileDirective]) -> Result<()> {
    for directive in directives.iter() {
        let output_path = self.install_dir.join(&directive.to);

        match directive {
            InlineFileDirective::InlineFile { source_data_id, .. } => {
                // Load data from extracted modlist folder
                let data = self.load_bytes_from_path(source_data_id).await?;
                std::fs::write(output_path, data)?;
            }
            InlineFileDirective::RemappedInlineFile { source_data_id, .. } => {
                // Load, remap paths, then write
                let mut data = String::from_utf8(self.load_bytes_from_path(source_data_id).await?)?;
                data = data.replace("{GAME_PATH}", &self.game_dir.display().to_string());
                data = data.replace("{INSTALL_PATH}", &self.install_dir.display().to_string());
                std::fs::write(output_path, data.as_bytes())?;
            }
        }
    }
}

async fn install(&self) -> Result<()> {
    self.extract_modlist().await?;           // Extract .wabbajack file
    self.prime_vfs().await?;                 // Build VFS index
    self.build_folder_structure().await?;    // Create directories

    self.install_archives().await?;          // VFS-based files
    self.install_included_files().await?;    // Non-VFS files (includes ModOrganizer.ini)

    self.write_meta_files().await?;          // .meta files for MO2
}