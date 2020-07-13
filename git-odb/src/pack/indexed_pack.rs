use crate::pack;
use git_object::{self as object, borrowed};
use quick_error::quick_error;
use std::{
    convert::TryFrom,
    path::{Path, PathBuf},
};

quick_error! {
    #[derive(Debug)]
    pub enum Error {
        InvalidPath(path: PathBuf) {
            display("An 'idx' extension is expected of an index file: '{}'", path.display())
        }
        Pack(err: pack::Error) {
            display("Could not instantiate pack")
            from()
            cause(err)
        }
        Index(err: pack::index::Error) {
            display("Could not instantiate pack index")
            from()
            cause(err)
        }
        Decode(err: pack::Error) {
            display("Could not decode object")
        }
    }
}

/// A packfile with an index
pub struct Bundle {
    pack: pack::File,
    index: pack::index::File,
}

impl Bundle {
    /// `path` is either a pack file or an index file
    pub fn at(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::try_from(path.as_ref())
    }

    /// `id` is a 20 byte SHA1 of the object to locate in the pack
    ///
    /// Note that ref deltas are automatically resolved within this pack only, which makes this implementation unusable
    /// for thin packs.
    /// For the latter, pack streams are required.
    pub fn locate<'a>(
        &self,
        id: &[u8],
        out: &'a mut Vec<u8>,
        cache: &mut impl pack::cache::DecodeEntry,
    ) -> Option<Result<Object<'a>, Error>> {
        let idx = self.index.lookup_index(id)?;
        let ofs = self.index.pack_offset_at_index(idx);
        let entry = self.pack.entry(ofs);
        self.pack
            .decode_entry(
                entry,
                out,
                |id, _out| {
                    self.index
                        .lookup_index(id)
                        .map(|idx| pack::ResolvedBase::InPack(self.pack.entry(self.index.pack_offset_at_index(idx))))
                },
                cache,
            )
            .map_err(Error::Decode)
            .map(move |r| Object {
                kind: r.kind,
                data: out.as_slice(),
            })
            .into()
    }
}

impl TryFrom<&Path> for Bundle {
    type Error = Error;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| Error::InvalidPath(path.to_owned()))?;
        Ok(match ext {
            "idx" => Self {
                index: pack::index::File::at(path)?,
                pack: pack::File::at(path.with_extension("pack"))?,
            },
            "pack" => Self {
                pack: pack::File::at(path)?,
                index: pack::index::File::at(path.with_extension("idx"))?,
            },
            _ => return Err(Error::InvalidPath(path.to_owned())),
        })
    }
}

pub struct Object<'data> {
    pub kind: git_object::Kind,
    pub data: &'data [u8],
}

impl<'data> Object<'data> {
    pub fn decode(&self) -> Result<borrowed::Object, borrowed::Error> {
        Ok(match self.kind {
            object::Kind::Tag => borrowed::Object::Tag(borrowed::Tag::from_bytes(self.data)?),
            object::Kind::Tree => borrowed::Object::Tree(borrowed::Tree::from_bytes(self.data)?),
            object::Kind::Commit => borrowed::Object::Commit(borrowed::Commit::from_bytes(self.data)?),
            object::Kind::Blob => borrowed::Object::Blob(borrowed::Blob { data: self.data }),
        })
    }
}