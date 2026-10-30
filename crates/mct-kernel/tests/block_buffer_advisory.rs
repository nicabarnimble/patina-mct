//! GHSA-qwgh-2vcv-g2f7 is about inline cursor corruption on unwind. That
//! representation and ReadBuffer arrived in 0.11, not this exact 0.10.4:
//! https://github.com/RustCrypto/utils/blob/block-buffer-v0.12.1/block-buffer/CHANGELOG.md
//! Keep the old-version classification narrow; 0.12.0 must instead be upgraded.
use block_buffer::{EagerBuffer, generic_array::typenum::U4};
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn legacy_buffer_compression_panic_does_not_overwrite_separate_cursor() {
    let mut buffer = EagerBuffer::<U4>::new(&[1, 2]);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            buffer.digest_blocks(&[3, 0xff], |_| panic!("compression failure"));
        }))
        .is_err()
    );
    assert_eq!(buffer.get_pos(), 2);
    assert_eq!(buffer.remaining(), 2);
    buffer.reset();
    buffer.digest_blocks(&[4], |_| unreachable!());
    assert_eq!(buffer.get_data(), &[4]);
}

#[test]
fn legacy_generation_panic_does_not_publish_invalid_cursor() {
    let mut buffer = EagerBuffer::<U4>::default();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            buffer.set_data(&mut [0u8; 1], |blocks| {
                if let Some(block) = blocks.first_mut() {
                    block[0] = 0xff;
                    panic!("generation failure");
                }
            });
        }))
        .is_err()
    );
    assert_eq!(buffer.get_pos(), 0);
    assert_eq!(buffer.remaining(), 4);
}
