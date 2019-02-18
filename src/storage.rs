use crate::{Debug, Outcome};
use std::fs;

#[derive(Clone, Debug)]
pub(crate) struct File<'a> {
    path: String,
    explorer: &'a dyn Explorer,
}

impl File<'_> {
    pub(crate) fn new(path: String) -> Self {
        if path.starts_with("MOCK://") {
            Self {path, explorer: &MockExplorer}
        } else {
            Self {path, explorer: &Local}
        }
    }

    pub(crate) fn read(&self) -> Outcome<String> {
        self.explorer.read(&self.path)
    }

    pub(crate) fn write(&self, data: &String) -> Outcome<()> {
        self.explorer.write(&self.path, data)
    }
}

impl Default for File<'_> {
    fn default() -> Self {
        Self {path: String::new(), explorer: &Local}
    }
}

trait Explorer: Debug {
    fn read(&self, path: &String) -> Outcome<String>;
    fn write(&self, path: &String, data: &String) -> Outcome<()>;
}

#[derive(Debug)]
struct Local;

impl Explorer for Local {
    fn read(&self, path: &String) -> Outcome<String> {
        Ok(fs::read_to_string(path).map(|data| data.replace('\r', ""))?)
    }

    fn write(&self, path: &String, data: &String) -> Outcome<()> {
        fs::write(path, data)?;
        Ok(())
    }
}

#[derive(Debug)]
struct MockExplorer;

impl Explorer for MockExplorer {
    fn read(&self, path: &String) -> Outcome<String> {
        Ok(String::new())
    }

    fn write(&self, path: &String, data: &String) -> Outcome<()> {
        Ok(())
    }
}
