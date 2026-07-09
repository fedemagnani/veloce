fn main() {
    // Suppose we have 3 slots, 1 reader:
    // - reader is reading slot 1
    // - the current is slot 2
    // - slot 0 is available

    let num_slots: u64 = 3;
    let busy_reader: u64 = 1;
    let current: u64 = 2;

    let mut forbidden: u64 = 1 << current;

    assert_eq!(
        forbidden,
        0b0000000000000000000000000000000000000000000000000000000000000100
    );

    forbidden |= 1 << busy_reader;

    assert_eq!(
        forbidden,
        0b0000000000000000000000000000000000000000000000000000000000000110
    );

    let mask: u64 = 1 << num_slots;

    assert_eq!(
        mask,
        0b0000000000000000000000000000000000000000000000000000000000001000
    );

    // set to 1 all the `num_slots` LSBs
    let mask = mask - 1;

    assert_eq!(
        mask,
        0b0000000000000000000000000000000000000000000000000000000000000111
    );

    // mark with 0 all the LSBs currently busy
    let available = !forbidden;

    assert_eq!(
        available,
        0b1111111111111111111111111111111111111111111111111111111111111001
    );

    // mark with 1 all the LSBs currently available
    let out = available & mask;

    assert_eq!(
        out,
        0b0000000000000000000000000000000000000000000000000000000000000001
    );

    // Find the position of the first LSB that is non-zero
    let free_index = out.trailing_zeros();

    assert_eq!(free_index, 0);

    let size = size_of::<u64>();
    println!("{size}");
}
