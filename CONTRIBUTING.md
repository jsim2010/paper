# CONTRIBUTING

The features/functionality of `paper` shall be described by its tests.

## New Features

Adding a new feature can be done by completing the following steps:

1. Write one or more test functions that check the success of the feature being implemented.
    - This should include rust documentation for the test function where the use case of the feature is explained.
    - If the desired functionality is not being implemented at the time the feature request is made, the test function must be annotated with the `ignore` attribute so that testing does not fail.
2. Implement the functionality to match the tests.
    - As functionality is implemented, remove the `ignore` attribute from the appropriate test function(s).

## Bug Fixes

Bugs are viewed as the application failing to implement a given feature - namely the feature that the described bug should not occur. Thus bug fixes shall follow the same process as adding a new feature. All added test functions should fail prior to any changes being made to the source code, thus confirming the new test(s) correctly identify the undesirable behavior.

## Coding Style

As many source code style conventions as possible shall be tested via lints or tests.

In addition, source code should adhere to the following recommendations which (unfortunately) cannot be automatically tested:
- Regular comments (i.e. not documentation comments) should only be used when they would assist a reasonably knowledgable reader in understanding why a section of code was written in its current form. A "reasonably knowledge reader" is a reader who has a solid understanding of the Rust language and a basic idea of how the program operates.
- Lints should be allowed sparingly and only after careful consideration (which must be described in a comment following the `allow` attribute). Lints may be allowed under the following conditions:
    + When a lint is not desired for any part of the project, it should be allowed at the beginning of `src/lib.rs`.
    + When the risks of allowing a lint are understood but they are acceptable or desired for the given context, the `allow` attribute for that lint should be added to the appropriate section of code. Code where a lint is allowed should be as consise as possible to avoid other cases of the lint being allowed unintentionally.
- The [Rust API Guidelines Checklist](https://rust-lang-nursery.github.io/api-guidelines/checklist.html) should be followed.
