use std::io::{self, Read};
use std::path::Path;
use std::time::SystemTime;

use algraf_data::{
    read_shapefile_bundle, read_shapefile_path, read_sqlite_path, read_sqlite_schema_path,
    ColumnDef, DataError, LoadResult, ShapefileBundle,
};

/// Minimal metadata the driver can ask an I/O provider for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverPathMetadata {
    pub len: u64,
    pub modified: Option<SystemTime>,
}

/// Bytes for the sidecars that make up an ESRI shapefile source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverShapefileBundle {
    pub shp: Vec<u8>,
    pub dbf: Vec<u8>,
    pub shx: Option<Vec<u8>>,
    pub prj: Option<Vec<u8>>,
    pub cpg: Option<Vec<u8>>,
}

impl DriverShapefileBundle {
    pub(crate) fn as_data_bundle(&self) -> ShapefileBundle<'_> {
        ShapefileBundle {
            shp: &self.shp,
            dbf: &self.dbf,
            shx: self.shx.as_deref(),
        }
    }
}

/// Synchronous I/O boundary for driver-owned data and schema loading.
///
/// The trait is deliberately limited to local bytes, stdin, file metadata, and
/// shapefile sidecar bundles. It has no network, process, environment, async,
/// or cache operations.
pub trait DriverIo {
    /// Read all bytes from a resolved data path.
    fn read_path(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// Read all bytes from standard input.
    fn read_stdin(&self) -> io::Result<Vec<u8>>;

    /// Return metadata for a resolved data path.
    fn metadata(&self, path: &Path) -> io::Result<DriverPathMetadata>;

    /// Read an optional path, treating only `NotFound` as absence.
    fn read_optional_path(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        match self.read_path(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Read a shapefile's sidecars relative to the named `.shp` path.
    fn read_shapefile_bundle(&self, path: &Path) -> io::Result<DriverShapefileBundle> {
        Ok(DriverShapefileBundle {
            shp: self.read_path(path)?,
            dbf: self.read_path(&path.with_extension("dbf"))?,
            shx: self.read_optional_path(&path.with_extension("shx"))?,
            prj: self.read_optional_path(&path.with_extension("prj"))?,
            cpg: self.read_optional_path(&path.with_extension("cpg"))?,
        })
    }

    /// Load a shapefile source from this provider.
    ///
    /// Custom providers use the sidecar bundle by default. The OS provider
    /// overrides this to preserve the `shapefile` crate's path-backed behavior
    /// and error surface exactly.
    fn load_shapefile(&self, path: &Path) -> Result<LoadResult, DataError> {
        let bundle = self.read_shapefile_bundle(path)?;
        read_shapefile_bundle(bundle.as_data_bundle())
    }

    /// Load a local SQLite query result from a resolved database path.
    fn load_sqlite(&self, path: &Path, query: &str) -> Result<LoadResult, DataError> {
        read_sqlite_path(path, query)
    }

    /// Load a bounded schema sample from a local SQLite query result.
    fn load_sqlite_schema(
        &self,
        path: &Path,
        query: &str,
        sample_size: usize,
    ) -> Result<Vec<ColumnDef>, DataError> {
        read_sqlite_schema_path(path, query, sample_size)
    }
}

impl<T: DriverIo + ?Sized> DriverIo for &T {
    fn read_path(&self, path: &Path) -> io::Result<Vec<u8>> {
        (**self).read_path(path)
    }

    fn read_stdin(&self) -> io::Result<Vec<u8>> {
        (**self).read_stdin()
    }

    fn metadata(&self, path: &Path) -> io::Result<DriverPathMetadata> {
        (**self).metadata(path)
    }

    fn read_optional_path(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        (**self).read_optional_path(path)
    }

    fn read_shapefile_bundle(&self, path: &Path) -> io::Result<DriverShapefileBundle> {
        (**self).read_shapefile_bundle(path)
    }

    fn load_shapefile(&self, path: &Path) -> Result<LoadResult, DataError> {
        (**self).load_shapefile(path)
    }

    fn load_sqlite(&self, path: &Path, query: &str) -> Result<LoadResult, DataError> {
        (**self).load_sqlite(path, query)
    }

    fn load_sqlite_schema(
        &self,
        path: &Path,
        query: &str,
        sample_size: usize,
    ) -> Result<Vec<ColumnDef>, DataError> {
        (**self).load_sqlite_schema(path, query, sample_size)
    }
}

/// Operating-system implementation used by compatibility wrappers.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsDriverIo;

impl DriverIo for OsDriverIo {
    fn read_path(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn read_stdin(&self) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn metadata(&self, path: &Path) -> io::Result<DriverPathMetadata> {
        let metadata = std::fs::metadata(path)?;
        Ok(DriverPathMetadata {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }

    fn load_shapefile(&self, path: &Path) -> Result<LoadResult, DataError> {
        read_shapefile_path(path)
    }
}
