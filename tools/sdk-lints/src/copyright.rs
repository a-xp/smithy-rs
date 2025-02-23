/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use anyhow::{bail, Result};

const EXPECTED_CONTENTS: &[&str] = &[
    "Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.",
    "SPDX-License-Identifier: Apache-2.0.",
];

const NEEDS_HEADER: [&str; 5] = ["sh", "py", "rs", "kt", "ts"];

pub(crate) fn check_copyright_header(path: impl AsRef<Path>) -> Result<()> {
    if !needs_copyright_header(path.as_ref()) {
        return Ok(());
    }
    let contents = match fs::read_to_string(path.as_ref()) {
        Ok(contents) => contents,
        Err(err) if format!("{}", err).contains("No such file or directory") => {
            eprintln!("Note: {} does not exist", path.as_ref().display());
            return Ok(());
        }
        Err(e) => return Err(e)?,
    };
    if !has_copyright_header(&contents) {
        bail!("{:?} is missing copyright header", path.as_ref())
    }
    Ok(())
}

fn needs_copyright_header(path: &Path) -> bool {
    let mut need_extensions = NEEDS_HEADER.iter().map(|s| OsStr::new(s));
    need_extensions.any(|extension| path.extension().unwrap_or_default() == extension)
}

fn has_copyright_header(contents: &str) -> bool {
    let mut expected = EXPECTED_CONTENTS.iter().peekable();
    // copyright header must be present in the first 10 lines
    for line in contents.lines().take(10) {
        match expected.peek() {
            Some(next) => {
                if line.contains(*next) {
                    let _ = expected.next();
                }
            }
            None => return true,
        }
    }
    expected.peek().is_none()
}

#[cfg(test)]
mod test {
    use crate::copyright::has_copyright_header;

    #[test]
    fn has_license_header() {
        let valid = [
            "// something else\n# Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.\n# SPDX-License-Identifier: Apache-2.0.",
            "# Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.\n# SPDX-License-Identifier: Apache-2.0.",
            "/*\n* Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.\n* SPDX-License-Identifier: Apache-2.0.\n */",
        ];

        let invalid = ["", "no license", "// something else\n# Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.\n# SPDX-License-Identifier: Apache-3.0."];

        for license in valid {
            assert!(
                has_copyright_header(license),
                "should be true: `{}`",
                license
            );
        }
        for license in invalid {
            assert!(
                !has_copyright_header(license),
                "should not be true: `{}`",
                license
            );
        }
    }
}
