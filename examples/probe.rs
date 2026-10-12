//! Read-only probe: lists the ATK HID interfaces and asks the command interface for a few values.
//! `cargo run --example probe`

use hidapi::HidApi;

fn req(cmd: u8, addr: u16, len: u8) -> Vec<u8> {
    let mut p = [0u8; 16];
    p[0] = cmd;
    p[2..4].copy_from_slice(&addr.to_be_bytes());
    p[4] = len;
    let sum: u32 = 8 + p[..15].iter().map(|&b| b as u32).sum::<u32>();
    p[15] = 85u8.wrapping_sub(sum as u8);
    let mut v = vec![8u8];
    v.extend_from_slice(&p);
    v
}

fn main() {
    let api = HidApi::new().unwrap();
    for d in api.device_list().filter(|d| d.vendor_id() == 0x373B) {
        println!(
            "{:04x}:{:04x} page={:04x} usage={} iface={} path={:?}",
            d.vendor_id(), d.product_id(), d.usage_page(), d.usage(), d.interface_number(), d.path()
        );
        if d.usage_page() != 0xFF02 {
            continue;
        }
        match d.open_device(&api) {
            Err(e) => println!("  open error: {e}"),
            Ok(dev) => {
                for (name, cmd, addr, len) in [("battery", 4u8, 0u16, 0u8), ("version", 18, 0, 0), ("cid/mid", 16, 0, 0), ("eeprom 0", 8, 0, 10)] {
                    println!("  {name}: write -> {:?}", dev.write(&req(cmd, addr, len)));
                    let mut b = [0u8; 64];
                    match dev.read_timeout(&mut b, 3000) {
                        Ok(n) => println!("  {name}: read {n}: {:?}", &b[..n]),
                        Err(e) => println!("  {name}: read error: {e}"),
                    }
                }
            }
        }
    }
}
