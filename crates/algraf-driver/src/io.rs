use std::future::Future;
use std::io::{self, Read};
use std::path::Path;
use std::pin::Pin;
use std::time::SystemTime;

use algraf_data::{
    read_shapefile_bundle, read_shapefile_path, DataError, LoadResult, ShapefileBundle,
};

pub type DriverIoFuture<'a, T> = Pin<Box<dyn Future<Output = io::Result<T>> + Send + 'a>>;
pub type DriverDataFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, DataError>> + Send + 'a>>;

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
}

/// Async-capable I/O boundary for callers that run data/schema loading without
/// blocking their request reactor (spec §10.8).
///
/// The async shape mirrors [`DriverIo`] deliberately: it can read resolved local
/// bytes, stdin, metadata, and shapefile sidecars, but it still has no network,
/// process, environment, or cache operations. Synchronous providers can be
/// exposed through [`BlockingAsyncDriverIo`].
pub trait AsyncDriverIo: Sync {
    fn read_path_async<'a>(&'a self, path: &'a Path) -> DriverIoFuture<'a, Vec<u8>>;

    fn read_stdin_async(&self) -> DriverIoFuture<'_, Vec<u8>>;

    fn metadata_async<'a>(&'a self, path: &'a Path) -> DriverIoFuture<'a, DriverPathMetadata>;

    fn read_optional_path_async<'a>(
        &'a self,
        path: &'a Path,
    ) -> DriverIoFuture<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            match self.read_path_async(path).await {
                Ok(bytes) => Ok(Some(bytes)),
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(err) => Err(err),
            }
        })
    }

    fn read_shapefile_bundle_async<'a>(
        &'a self,
        path: &'a Path,
    ) -> DriverIoFuture<'a, DriverShapefileBundle> {
        Box::pin(async move {
            let dbf_path = path.with_extension("dbf");
            let shx_path = path.with_extension("shx");
            let prj_path = path.with_extension("prj");
            let cpg_path = path.with_extension("cpg");
            Ok(DriverShapefileBundle {
                shp: self.read_path_async(path).await?,
                dbf: self.read_path_async(&dbf_path).await?,
                shx: self.read_optional_path_async(&shx_path).await?,
                prj: self.read_optional_path_async(&prj_path).await?,
                cpg: self.read_optional_path_async(&cpg_path).await?,
            })
        })
    }

    fn load_shapefile_async<'a>(&'a self, path: &'a Path) -> DriverDataFuture<'a, LoadResult> {
        Box::pin(async move {
            let bundle = self.read_shapefile_bundle_async(path).await?;
            read_shapefile_bundle(bundle.as_data_bundle())
        })
    }
}

/// Async adapter for an existing synchronous provider.
#[derive(Debug, Clone, Copy)]
pub struct BlockingAsyncDriverIo<I> {
    inner: I,
}

impl<I> BlockingAsyncDriverIo<I> {
    pub fn new(inner: I) -> BlockingAsyncDriverIo<I> {
        BlockingAsyncDriverIo { inner }
    }

    pub fn inner(&self) -> &I {
        &self.inner
    }
}

impl<I: DriverIo + Sync> AsyncDriverIo for BlockingAsyncDriverIo<I> {
    fn read_path_async<'a>(&'a self, path: &'a Path) -> DriverIoFuture<'a, Vec<u8>> {
        let path = path.to_path_buf();
        Box::pin(async move { self.inner.read_path(&path) })
    }

    fn read_stdin_async(&self) -> DriverIoFuture<'_, Vec<u8>> {
        Box::pin(async move { self.inner.read_stdin() })
    }

    fn metadata_async<'a>(&'a self, path: &'a Path) -> DriverIoFuture<'a, DriverPathMetadata> {
        let path = path.to_path_buf();
        Box::pin(async move { self.inner.metadata(&path) })
    }

    fn read_optional_path_async<'a>(
        &'a self,
        path: &'a Path,
    ) -> DriverIoFuture<'a, Option<Vec<u8>>> {
        let path = path.to_path_buf();
        Box::pin(async move { self.inner.read_optional_path(&path) })
    }

    fn read_shapefile_bundle_async<'a>(
        &'a self,
        path: &'a Path,
    ) -> DriverIoFuture<'a, DriverShapefileBundle> {
        let path = path.to_path_buf();
        Box::pin(async move { self.inner.read_shapefile_bundle(&path) })
    }

    fn load_shapefile_async<'a>(&'a self, path: &'a Path) -> DriverDataFuture<'a, LoadResult> {
        let path = path.to_path_buf();
        Box::pin(async move { self.inner.load_shapefile(&path) })
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
