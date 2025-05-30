use super::*;

#[test]
fn abracadabra() {
    let s = b"abracadabra";
    let lcp = LcpArray::new(s);
    let naive = LcpArray::naive(s);
    assert_eq!(lcp.sa(), naive.sa());
    assert_eq!(lcp.sa(), &[10, 7, 0, 3, 5, 8, 1, 4, 6, 9, 2]);
    assert_eq!(lcp.lcp(), naive.lcp());
}

#[test]
fn mississippi() {
    let s = b"mississippi";
    let lcp = LcpArray::new(s);
    let naive = LcpArray::naive(s);
    assert_eq!(lcp.sa(), naive.sa());
    assert_eq!(lcp.sa(), &[10, 7, 4, 1, 0, 9, 8, 6, 3, 5, 2]);
    assert_eq!(lcp.lcp(), naive.lcp());
}

#[test]
fn fib() {
    let s = fib_str(b'a', b'b', 1024);
    let lcp = LcpArray::new(&s);
    let naive = LcpArray::naive(&s);
    assert_eq!(lcp.sa(), naive.sa());
    assert_eq!(lcp.lcp(), naive.lcp());
}

#[allow(unused)]
fn print_sa_lcp(s: &[u8], lcp: &LcpArray) {
    println!("input: {:?}", unsafe { core::str::from_utf8_unchecked(s) });
    fn print_suffix(s: &[u8], suffix: u32) {
        let Some(s) = s.get(suffix as usize..) else {
            unreachable!();
        };
        println!("{:3}: {:?}", suffix, unsafe {
            core::str::from_utf8_unchecked(&s)
        });
    }

    for (index, (&lcp, &suffix)) in lcp.lcp().iter().zip(lcp.sa().iter()).enumerate() {
        print_suffix(s, suffix);
        if index < s.len() - 1 {
            println!("   +-- lcp {}", lcp);
        }
    }

    for (index, &rank) in lcp.rank().iter().enumerate() {
        let suffix = lcp.sa()[rank as usize];
        let lcp = lcp.lcp()[rank as usize];
        println!("rank[{:3}] = {:3} {:3}", index, rank, lcp);
        assert_eq!(suffix, index as u32);
    }
}
