////////////////////////////////////////////////////////////////////
// byte row-collection module
////////////////////////////////////////////////////////////////////

use serde::{Deserialize, Serialize};

use crate::fields::Field;
use crate::row_collection::RowCollection;
use crate::row_metadata::RowMetadata;
use crate::rows::Row;
use crate::table_columns::TableColumn;
use crate::typed_values::TypedValue;

/// Byte-vector-based RowCollection implementation
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ByteRowCollection {
    columns: Vec<TableColumn>,
    row_data: Vec<Vec<u8>>,
    record_size: usize,
    watermark: usize,
}

impl ByteRowCollection {
    /// Decodes a byte vector into a [ByteRowCollection]
    pub fn decode(columns: Vec<TableColumn>, bytes: Vec<u8>) -> Self {
        let record_size = Row::compute_record_size(&columns);
        let row_bytes = bytes.chunks(record_size)
            .map(|chunk| chunk.to_vec())
            .collect();
        Self::new(columns, row_bytes)
    }

    /// Encodes the [ByteRowCollection] into a byte vector
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = vec![];
        for row in &self.row_data { bytes.extend(row) }
        bytes
    }

    /// Creates a new [ByteRowCollection] from the specified rows
    pub fn from_rows(rows: Vec<Row>) -> Self {
        let mut encoded_rows = vec![];
        let columns = rows.first()
            .map(|row| row.get_columns().clone())
            .unwrap_or(vec![]);
        for row in rows { encoded_rows.push(row.encode()) }
        Self::new(columns, encoded_rows)
    }

    pub fn get_rows(&self) -> Vec<Row> {
        let mut rows = vec![];
        for buf in &self.row_data {
            let (row, rmd) = Row::decode(&buf, &self.columns);
            if rmd.is_allocated { rows.push(row) }
        }
        rows
    }

    /// Creates a new [ByteRowCollection] from the specified row data
    pub fn new(columns: Vec<TableColumn>, rows: Vec<Vec<u8>>) -> Self {
        ByteRowCollection {
            record_size: Row::compute_record_size(&columns),
            watermark: rows.len(),
            columns,
            row_data: rows,
        }
    }
}

impl RowCollection for ByteRowCollection {
    /// Returns the collection's columns
    fn get_columns(&self) -> &Vec<TableColumn> { &self.columns }

    /// Returns the width of the record
    fn get_record_size(&self) -> usize { self.record_size }

    /// Returns the number of rows in the collection
    fn len(&self) -> std::io::Result<usize> { Ok(self.watermark) }

    /// Overwrites a row by ID
    fn overwrite(&mut self, id: usize, row: &Row) -> std::io::Result<usize> {
        // resize the rows to prevent overflow
        if self.row_data.len() <= id {
            self.row_data.resize(id + 1, vec![]);
        }

        // set the block, update the watermark
        self.row_data[id] = row.encode();
        if self.watermark <= id {
            self.watermark = id + 1;
        }
        Ok(1)
    }

    /// Overwrites the row metadata by ID
    fn overwrite_metadata(&mut self, id: usize, metadata: &RowMetadata) -> std::io::Result<usize> {
        self.row_data[id][0] = metadata.encode();
        Ok(1)
    }

    /// Reads a row by ID
    fn read(&self, id: usize) -> std::io::Result<(Row, RowMetadata)> {
        Ok(Row::decode(&self.row_data[id], &self.columns))
    }

    /// Reads a field by row ID and column ID
    fn read_field(&self, id: usize, column_id: usize) -> std::io::Result<TypedValue> {
        let column = &self.columns[column_id];
        let buffer = self.row_data[id][column.offset..(column.offset + column.max_physical_size)].to_vec();
        let field = Field::decode(&column.data_type, &buffer, 0);
        Ok(field.value)
    }

    /// Reads a range of rows
    fn read_range(&self, index: std::ops::Range<usize>) -> std::io::Result<Vec<Row>> {
        let rows = self.row_data[index].iter().flat_map(|b| {
            let (row, meta) = Row::decode(b, &self.columns);
            if meta.is_allocated { Some(row) } else { None }
        }).collect();
        Ok(rows)
    }

    /// Resize a range of rows
    fn resize(&mut self, new_size: usize) -> std::io::Result<()> {
        self.row_data.resize(new_size, vec![]);
        self.watermark = new_size;
        Ok(())
    }
}

// Unit tests
#[cfg(test)]
mod tests {
    use crate::byte_row_collection::ByteRowCollection;
    use crate::table_columns::TableColumn;
    use crate::testdata::{make_columns, make_quote};

    #[test]
    fn test_encode_decode() {
        let columns = make_columns();
        let phys_columns = TableColumn::from_columns(&columns).unwrap();
        let mrc = ByteRowCollection::from_rows(vec![
            make_quote(0, &phys_columns, "ABC", "AMEX", 12.33),
            make_quote(1, &phys_columns, "UNO", "OTC", 0.2456),
            make_quote(2, &phys_columns, "BIZ", "NYSE", 9.775),
            make_quote(3, &phys_columns, "GOTO", "OTC", 0.1442),
            make_quote(4, &phys_columns, "XYZ", "NYSE", 0.0289),
        ]);
        let encoded = mrc.encode();
        assert_eq!(ByteRowCollection::decode(phys_columns, encoded), mrc)
    }

    #[test]
    fn test_get_rows() {
        let columns = make_columns();
        let phys_columns = TableColumn::from_columns(&columns).unwrap();
        let mrc = ByteRowCollection::from_rows(vec![
            make_quote(0, &phys_columns, "ABC", "AMEX", 12.33),
            make_quote(1, &phys_columns, "UNO", "OTC", 0.2456),
            make_quote(2, &phys_columns, "BIZ", "NYSE", 9.775),
            make_quote(3, &phys_columns, "GOTO", "OTC", 0.1442),
            make_quote(4, &phys_columns, "XYZ", "NYSE", 0.0289),
        ]);
        assert_eq!(mrc.get_rows(), vec![
            make_quote(0, &phys_columns, "ABC", "AMEX", 12.33),
            make_quote(1, &phys_columns, "UNO", "OTC", 0.2456),
            make_quote(2, &phys_columns, "BIZ", "NYSE", 9.775),
            make_quote(3, &phys_columns, "GOTO", "OTC", 0.1442),
            make_quote(4, &phys_columns, "XYZ", "NYSE", 0.0289),
        ])
    }
}