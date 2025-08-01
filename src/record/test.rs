use std::{mem, string::ToString, sync::Arc};

use arrow::{
    array::{
        Array, AsArray, BooleanArray, BooleanBufferBuilder, RecordBatch, StringArray,
        StringBuilder, UInt32Array, UInt32Builder,
    },
    datatypes::{DataType, Field, Schema as ArrowSchema, UInt32Type},
};
use once_cell::sync::Lazy;
use parquet::{arrow::ProjectionMask, format::SortingColumn, schema::types::ColumnPath};

use super::{
    option::OptionRecordRef, ArrowArrays, ArrowArraysBuilder, Key, Record, RecordRef, Schema,
};
use crate::{magic, version::timestamp::Ts};

const PRIMARY_FIELD_NAME: &str = "vstring";

#[derive(Debug)]
pub struct StringSchema;

impl Schema for StringSchema {
    type Record = String;

    type Columns = StringColumns;

    type Key = String;

    fn arrow_schema(&self) -> &Arc<ArrowSchema> {
        static SCHEMA: Lazy<Arc<ArrowSchema>> = Lazy::new(|| {
            Arc::new(ArrowSchema::new(vec![
                Field::new("_null", DataType::Boolean, false),
                Field::new(magic::TS, DataType::UInt32, false),
                Field::new(PRIMARY_FIELD_NAME, DataType::Utf8, false),
            ]))
        });

        &SCHEMA
    }

    fn primary_key_index(&self) -> usize {
        2
    }

    fn primary_key_path(&self) -> (ColumnPath, Vec<SortingColumn>) {
        (
            ColumnPath::new(vec![magic::TS.to_string(), PRIMARY_FIELD_NAME.to_string()]),
            vec![
                SortingColumn::new(1, true, true),
                SortingColumn::new(2, false, true),
            ],
        )
    }
}

impl Record for String {
    type Schema = StringSchema;

    type Ref<'r>
        = &'r str
    where
        Self: 'r;

    fn key(&self) -> &str {
        self
    }

    fn as_record_ref(&self) -> Self::Ref<'_> {
        self
    }

    fn size(&self) -> usize {
        self.len()
    }
}

impl<'r> RecordRef<'r> for &'r str {
    type Record = String;

    fn key(self) -> <<<Self::Record as Record>::Schema as Schema>::Key as Key>::Ref<'r> {
        self
    }

    fn projection(&mut self, _: &ProjectionMask) {}

    fn from_record_batch(
        record_batch: &'r RecordBatch,
        offset: usize,
        _: &'r ProjectionMask,
        _: &'r Arc<ArrowSchema>,
    ) -> OptionRecordRef<'r, Self> {
        let ts = record_batch
            .column(1)
            .as_primitive::<UInt32Type>()
            .value(offset)
            .into();
        let vstring = record_batch.column(2).as_string::<i32>().value(offset);
        let null = record_batch.column(0).as_boolean().value(offset);

        OptionRecordRef::new(ts, vstring, null)
    }
}

#[derive(Debug)]
pub struct StringColumns {
    _null: Arc<BooleanArray>,
    _ts: Arc<UInt32Array>,
    string: Arc<StringArray>,

    record_batch: RecordBatch,
}

impl ArrowArrays for StringColumns {
    type Record = String;

    type Builder = StringColumnsBuilder;

    fn builder(_schema: Arc<ArrowSchema>, capacity: usize) -> Self::Builder {
        StringColumnsBuilder {
            _null: BooleanBufferBuilder::new(capacity),
            _ts: UInt32Builder::with_capacity(capacity),
            string: StringBuilder::with_capacity(capacity, 0),
        }
    }

    fn get(
        &self,
        offset: u32,
        _: &ProjectionMask,
    ) -> Option<Option<<Self::Record as Record>::Ref<'_>>> {
        if offset as usize >= self.string.len() {
            return None;
        }

        if self._null.value(offset as usize) {
            return Some(None);
        }

        Some(Some(self.string.value(offset as usize)))
    }

    fn as_record_batch(&self) -> &RecordBatch {
        &self.record_batch
    }
}

#[derive(Debug)]
pub struct StringColumnsBuilder {
    _null: BooleanBufferBuilder,
    _ts: UInt32Builder,
    string: StringBuilder,
}

impl ArrowArraysBuilder<StringColumns> for StringColumnsBuilder {
    fn push(&mut self, key: Ts<&str>, row: Option<&str>) {
        self._null.append(row.is_none());
        self._ts.append_value(key.ts.into());
        if let Some(row) = row {
            self.string.append_value(row);
        } else {
            self.string.append_value(String::default());
        }
    }

    fn written_size(&self) -> usize {
        self._null.as_slice().len()
            + mem::size_of_val(self._ts.values_slice())
            + mem::size_of_val(self.string.values_slice())
    }

    fn finish(&mut self, _: Option<&[usize]>) -> StringColumns {
        let _null = Arc::new(BooleanArray::new(self._null.finish(), None));
        let _ts = Arc::new(self._ts.finish());
        let string = Arc::new(self.string.finish());

        let schema = StringSchema;
        let record_batch = RecordBatch::try_new(
            schema.arrow_schema().clone(),
            vec![
                Arc::clone(&_null) as Arc<dyn Array>,
                Arc::clone(&_ts) as Arc<dyn Array>,
                Arc::clone(&string) as Arc<dyn Array>,
            ],
        )
        .unwrap();

        StringColumns {
            _null,
            _ts,
            string,
            record_batch,
        }
    }
}
