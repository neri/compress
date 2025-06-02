use super::{lcp::LcpArrayNaive, *};

#[test]
fn abracadabra() {
    let s = b"abracadabra";
    let lcp = MatchFinder::new(s);
    let naive = LcpArrayNaive::new(s);
    assert_eq!(lcp.sa(), naive.sa());
    assert_eq!(lcp.sa(), &[10, 7, 0, 3, 5, 8, 1, 4, 6, 9, 2]);
    assert_eq!(lcp.lcp(), naive.lcp());
}

#[test]
fn mississippi() {
    let s = b"mississippi";
    let lcp = MatchFinder::new(s);
    let naive = LcpArrayNaive::new(s);
    assert_eq!(lcp.sa(), naive.sa());
    assert_eq!(lcp.sa(), &[10, 7, 4, 1, 0, 9, 8, 6, 3, 5, 2]);
    assert_eq!(lcp.lcp(), naive.lcp());
}

#[test]
fn fib() {
    let s = fib_str(b'a', b'b', 0x1000);
    let lcp = MatchFinder::new(&s);
    let naive = LcpArrayNaive::new(&s);
    assert_eq!(lcp.sa(), naive.sa());
    assert_eq!(lcp.lcp(), naive.lcp());
}

#[test]
fn random() {
    let s = random_bytes(0x55, 0xaa, 0x1000);
    let lcp = MatchFinder::new(&s);
    let naive = LcpArrayNaive::new(&s);
    assert_eq!(lcp.sa(), naive.sa());
    assert_eq!(lcp.lcp(), naive.lcp());
}

#[allow(unused)]
fn print_sa_lcp(s: &[u8], lcp: &MatchFinder) {
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

    for (index, &rank) in lcp.rev_sa().iter().enumerate() {
        let suffix = lcp.sa()[rank as usize];
        let lcp = lcp.lcp()[rank as usize];
        println!("rank[{:3}] = {:3} {:3}", index, rank, lcp);
        assert_eq!(suffix, index as u32);
    }
}
