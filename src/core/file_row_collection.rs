#![warn(dead_code)]
////////////////////////////////////////////////////////////////////
// file row-collection module
////////////////////////////////////////////////////////////////////

use crate::blobs::{BLOBCellMetadata, BLOBStore};
use crate::byte_code_compiler::ByteCodeCompiler;
use crate::columns::Column;
use crate::data_types::DataType::NumberType;
use crate::errors::{throw, Errors};
use crate::field;
use crate::field::FieldMetadata;
use crate::machine::Machine;
use crate::namespaces::Namespace;
use crate::number_kind::NumberKind::U64Kind;
use crate::numbers::Numbers;
use crate::object_config::ObjectConfig;
use crate::parameter::Parameter;
use crate::platform::PlatformOps;
use crate::row_collection::{RowCollection, RowEncoding};
use crate::row_metadata::RowMetadata;
use crate::structures::Row;
use crate::typed_values::TypedValue;
use crate::typed_values::TypedValue::{ErrorValue, Number};
use log::error;
use serde::de::Error;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use shared_lib::fail;
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::fs;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::sync::Arc;

/// File-based RowCollection implementation
#[derive(Clone)]
pub struct FileRowCollection {
    blobs: BLOBStore,
    columns: Vec<Column>,
    file: Arc<File>,
    path: String,
    record_size: usize,
}

impl FileRowCollection {
    pub fn build(
        columns: Vec<Column>,
        path: &str,
    ) -> std::io::Result<Self> {
        let full_blob_path = format!("{}.blob", path);
        let blobs = BLOBStore::open_file(full_blob_path.as_str(), true).unwrap();
        Ok(Self {
            record_size: Row::compute_record_size(&columns),
            columns,
            blobs,
            file: Arc::from(File::open(path)?),
            path: path.to_string(),
        })
    }

    /// Creates a new table within the specified namespace and having the specified columns
    pub fn create_table(ns: &Namespace, params: &Vec<Parameter>) -> std::io::Result<Self> {
        let path = ns.get_table_file_path();
        let columns = Column::from_parameters(params);
        ObjectConfig::build_table(params.clone()).save(ns)?;
        let file = Arc::new(Self::table_file_create(ns)?);
        Ok(Self::new(columns, file, path.as_str()))
    }

    pub fn get_related_filename(path: &str, extension: &str) -> (String, String) {
        let (oxide_home, untitled) = (Machine::oxide_home(), "untitled");
        let raw_file_path = Path::new(path);

        // determine the index_name and full_path
        let base_path = raw_file_path.parent()
            .map(|p| p.to_str().unwrap_or(oxide_home.as_str()))
            .unwrap_or(oxide_home.as_str());
        let table_filename = raw_file_path.file_name()
            .map(|p| p.to_str().unwrap_or(untitled))
            .unwrap_or(untitled);
        let index_filename = match table_filename.find(".") {
            Some(n) => format!("{}.{}", &table_filename[0..n], extension),
            None => format!("{}.{}", untitled, extension)
        };
        let full_path = format!("{}/{}", base_path, index_filename);
        (base_path.to_string(), full_path)
    }

    pub fn new(
        columns: Vec<Column>,
        file: Arc<File>,
        path: &str,
    ) -> Self {
        let full_blob_path = format!("{}.blob", path);
        let blobs = BLOBStore::open_file(full_blob_path.as_str(), true).unwrap();
        Self {
            record_size: Row::compute_record_size(&columns),
            columns,
            blobs,
            file,
            path: path.to_string(),
        }
    }

    pub fn open(ns: &Namespace) -> std::io::Result<Self> {
        Self::open_file(ns, Self::table_file_open(&ns)?)
    }

    fn open_file(ns: &Namespace, file: File) -> std::io::Result<Self> {
        let cfg = ObjectConfig::load(&ns)?;
        let path = ns.get_table_file_path();
        let columns = Column::from_parameters(&cfg.get_columns());
        Ok(Self::new(columns, Arc::new(file), path.as_str()))
    }

    pub fn open_or_create(ns: &Namespace, params: Vec<Parameter>) -> std::io::Result<Self> {
        match Self::table_file_open(&ns) {
            Ok(file) => Self::open_file(ns, file),
            Err(err) if err.to_string().starts_with("No such file") => {
                match Self::table_file_create(&ns) {
                    Ok(file) => {
                        let cfg = ObjectConfig::build_table(params);
                        cfg.save(&ns)?;
                        Self::open_file(ns, file)
                    }
                    Err(err) => fail(err.to_string())
                }
            }
            Err(err) => fail(err.to_string())
        }
    }

    /// convenience function to create, read or write a table file
    pub(crate) fn table_file_create(ns: &Namespace) -> std::io::Result<File> {
        fs::create_dir_all(ns.get_root_path())?;
        OpenOptions::new().truncate(true).create(true).read(true).write(true)
            .open(ns.get_table_file_path())
    }

    /// convenience function to read or write a table file
    pub(crate) fn table_file_open(ns: &Namespace) -> std::io::Result<File> {
        OpenOptions::new().read(true).write(true)
            .open(ns.get_table_file_path())
    }
}

impl Eq for FileRowCollection {}

impl Ord for FileRowCollection {
    fn cmp(&self, other: &Self) -> Ordering {
        self.record_size.cmp(&other.record_size)
    }
}

impl PartialEq for FileRowCollection {
    fn eq(&self, other: &Self) -> bool {
        self.record_size == other.record_size
    }
}

impl PartialOrd for FileRowCollection {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.record_size.partial_cmp(&other.record_size)
    }
}

impl Debug for FileRowCollection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "FileRowCollection({})", self.record_size)
    }
}

impl RowCollection for FileRowCollection {
    fn create_related_structure(
        &self,
        columns: Vec<Column>,
        extension: &str,
    ) -> std::io::Result<Box<dyn RowCollection>> {
        // determine the base_path and full_path
        let path = self.path.as_str();
        let (base_path, full_path) =
            Self::get_related_filename(path, extension);

        // ensure the parent (base) directory exists
        fs::create_dir_all(base_path)?;

        // create and/or open the file
        let file = OpenOptions::new().truncate(true).create(true).read(true).write(true).open(full_path)?;
        let frc = Self::new(columns, Arc::new(file), path);
        Ok(Box::new(frc))
    }

    fn get_columns(&self) -> &Vec<Column> { &self.columns }

    fn get_record_size(&self) -> usize { self.record_size }

    fn get_rows(&self) -> Vec<Row> {
        self.iter().collect()
    }

    fn len(&self) -> std::io::Result<usize> {
        Ok((self.file.metadata()?.len() as usize) / self.record_size)
    }

    fn overwrite_field(
        &mut self,
        id: usize,
        column_id: usize,
        new_value: TypedValue,
    ) -> TypedValue {
        let column = &self.columns[column_id];
        let offset = self.convert_rowid_to_offset(id) + column.get_offset() as u64;
        let buffer = self.blobs.encode_field(&column, &new_value)
            .unwrap_or_else(|err| {
                error!("Failed to write to {}@({id}, {column_id}): {} ({})", column.get_name(), err, new_value);
                Self::empty_cell(column)
            });
        TypedValue::from_result(self.write_at(offset, &buffer).map(|n| Number(n)))
    }

    fn overwrite_field_metadata(
        &mut self,
        id: usize,
        column_id: usize,
        metadata: FieldMetadata,
    ) -> TypedValue {
        let row_offset = self.convert_rowid_to_offset(id);
        let column = &self.columns[column_id];
        let column_offset = column.get_offset() as u64;
        let cell_offset = row_offset + column_offset;
        TypedValue::from_result(self.write_at(cell_offset, &[metadata.encode()].to_vec())
            .map(|n| Number(n)))
    }

    fn overwrite_row(&mut self, id: usize, row: Row) -> TypedValue {
        let row_offset = self.convert_rowid_to_offset(id);
        let capacity = self.get_record_size();
        let blobs = &self.blobs;

        // encode the row => (metadata|row ID|data)
        let mut encoded = Vec::with_capacity(capacity);
        encoded.push(RowMetadata::new(true).encode());
        encoded.extend(ByteCodeCompiler::encode_row_id(row.get_id()));
        encoded.extend(self.columns.iter().zip(row.get_values().iter())
            .flat_map(|(column, value)|
                blobs.encode_field(column, value).unwrap_or_else(|err| {
                    error!("Failed to write row #{id}: {err} ({})", row.to_json_string(&self.columns));
                    vec![]
                })
            ).collect::<Vec<_>>());
        encoded.resize(capacity, 0u8);

        // write the row
        TypedValue::from_result(self.write_at(row_offset, &encoded)
            .map(|n| Number(n)))
    }

    fn overwrite_row_metadata(&mut self, id: usize, metadata: RowMetadata) -> TypedValue {
        let row_offset = self.convert_rowid_to_offset(id);
        TypedValue::from_result(self.write_at(row_offset, &[metadata.encode()].to_vec())
            .map(|n| Number(n)))
    }

    fn read_field(&self, id: usize, column_id: usize) -> TypedValue {
        let column = &self.columns[column_id];
        let row_offset = self.convert_rowid_to_offset(id);
        let cell_offset = row_offset + column.get_offset() as u64;
        match self.read_at(cell_offset, column.get_fixed_size()) {
            Ok(buffer) => column.get_data_type().decode_field_value(&buffer, 0),
            Err(err) => ErrorValue(Errors::Exact(err.to_string()))
        }
    }

    fn read_field_metadata(
        &self,
        id: usize,
        column_id: usize,
    ) -> std::io::Result<FieldMetadata> {
        let column = &self.columns[column_id];
        let row_offset = self.convert_rowid_to_offset(id);
        let cell_offset = row_offset + column.get_offset() as u64;
        let buffer = self.read_at(cell_offset, 1)?;
        let meta = FieldMetadata::decode(buffer[0]);
        Ok(meta)
    }

    fn read_row(&self, id: usize) -> std::io::Result<(Row, RowMetadata)> {
        let row_offset = self.convert_rowid_to_offset(id);
        let buffer = self.read_at(row_offset, self.record_size)?;
        let columns = &self.columns;

        // if the buffer is empty, just return an empty row
        if buffer.len() == 0 {
            return Ok((Row::create(0, columns), RowMetadata::new(false)));
        }
        let rmd = RowMetadata::from_bytes(&buffer, 0);
        let id = ByteCodeCompiler::decode_row_id(&buffer, 1);
        let values = columns.iter().map(|column| {
            let fmd = FieldMetadata::decode(buffer[column.get_offset()]);
            if fmd.is_external {
                let offset = NumberType(U64Kind).decode_field_value(&buffer, column.get_offset()).to_u64();
                let (_, value) = self.blobs.read(offset)
                    .unwrap_or_else(|err| (BLOBCellMetadata::new(0, 0, 0), ErrorValue(Errors::Exact(err.to_string()))));
                value
            } else {
                let data_type = column.get_data_type();
                data_type.decode_field_value(&buffer, column.get_offset())
            }
        }).collect();
        Ok((Row::new(id, values), rmd))
    }

    fn read_row_metadata(&self, id: usize) -> std::io::Result<RowMetadata> {
        let row_offset = self.convert_rowid_to_offset(id);
        let buffer = self.read_at(row_offset, 1)?;
        Ok(RowMetadata::decode(buffer[0]))
    }

    fn resize(&mut self, new_size: usize) -> TypedValue {
        let new_length = new_size as u64 * self.record_size as u64;
        match self.file.set_len(new_length) {
            Ok(..) => Number(Numbers::Ack),
            Err(err) => ErrorValue(Errors::Exact(err.to_string()))
        }
    }
}

impl RowEncoding for FileRowCollection {
    fn read_at(&self, offset: u64, count: usize) -> std::io::Result<Vec<u8>> {
        let mut buffer: Vec<u8> = vec![0u8; count];
        match self.file.read_at(&mut buffer, offset) {
            Ok(_n_bytes) => Ok(buffer),
            Err(err) => throw(Errors::Exact(err.to_string()))
        }
    }

    fn write_at(&self, offset: u64, bytes: &Vec<u8>) -> std::io::Result<Numbers> {
        let _n_bytes = self.file.write_at(bytes.as_slice(), offset)?;
        Ok(Numbers::RowsAffected(1))
    }
}

impl Serialize for FileRowCollection {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("FileRowCollection", 2)?;
        state.serialize_field("columns", &self.columns)?;
        state.serialize_field("path", &self.path)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for FileRowCollection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // define a helper struct for deserialization
        #[derive(Deserialize)]
        struct FileRowCollectionHelper {
            columns: Vec<Column>,
            path: String,
        }

        let helper = FileRowCollectionHelper::deserialize(deserializer)?;
        let file = File::open(&helper.path).map_err(D::Error::custom)?;
        Ok(FileRowCollection::new(helper.columns, Arc::new(file), helper.path.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use crate::file_row_collection::FileRowCollection;
    use crate::namespaces::Namespace;
    use crate::numbers::Numbers::F64Value;
    use crate::row_collection::RowCollection;
    use crate::structures::Row;
    use crate::testdata::make_quote_parameters;
    use crate::typed_values::TypedValue::{Number, StringValue};

    #[test]
    fn test_column_overflow() {
        let mut frc = create_file_row_collection("frc.overflow.stocks");
        let row0 = Row::new(0, vec![
            StringValue("VERY_LONG_SYMBOL".into()),
            StringValue("NYSE".into()),
            Number(F64Value(12.13))
        ]);
        frc.append_row(row0.clone());
        let (row1, _) = frc.read_row(0).unwrap();
        assert_eq!(row0, row1)
    }

    fn create_file_row_collection(path: &str) -> FileRowCollection {
        FileRowCollection::create_table(
            &Namespace::parse(path).unwrap(),
            &make_quote_parameters(),
        ).unwrap()
    }
}
