use crate::backup;
use crate::protocol::*;
use hidapi::{HidApi, HidDevice};

const VID: u16 = 0x373B;
const USAGE_PAGE: u16 = 0xFF02;

pub struct Mouse {
    dev: HidDevice,
}

impl Mouse {
    pub fn open() -> Result<Self, String> {
        let api = HidApi::new().map_err(|e| e.to_string())?;
        let info = api
            .device_list()
            .find(|d| d.vendor_id() == VID && d.usage_page() == USAGE_PAGE && d.usage() == 2)
            .ok_or("ATK receiver not found. Plug in the dongle and close ATK Hub in the browser.")?;
        let dev = info.open_device(&api).map_err(|e| format!("Could not open the receiver: {e}"))?;
        Ok(Self { dev })
    }

    /// Sends a packet and waits for the reply that echoes the command id. Retries if the mouse is asleep.
    fn exchange(&self, packet: [u8; PACKET_LEN]) -> Result<Vec<u8>, String> {
        let cmd = packet[0];
        let mut out = vec![REPORT_ID];
        out.extend_from_slice(&packet);
        let mut buf = [0u8; 64];
        for _ in 0..3 {
            self.dev.write(&out).map_err(|e| e.to_string())?;
            for _ in 0..3 {
                let n = self.dev.read_timeout(&mut buf, 700).map_err(|e| e.to_string())?;
                if n == 0 {
                    continue;
                }
                // with numbered reports Windows puts the report id in the first byte
                let r = if buf[0] == REPORT_ID { &buf[1..n] } else { &buf[..n] };
                if r.first() == Some(&cmd) {
                    return Ok(r.to_vec());
                }
            }
        }
        Err(format!("No response from the mouse (command {cmd}). Move the mouse to wake it up and try again"))
    }

    /// Simple command without an address; returns the reply data.
    pub fn cmd(&self, cmd: u8, len: u8, data: &[u8]) -> Result<Vec<u8>, String> {
        let r = self.exchange(build(cmd, 0, len, data))?;
        Ok(payload(&r, CHUNK).ok_or("Incomplete reply")?.to_vec())
    }

    /// Reads `len` bytes from the EEPROM, 10 at a time.
    pub fn read(&self, addr: u16, len: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let n = (len - out.len()).min(CHUNK);
            let a = addr + out.len() as u16;
            let r = self.exchange(request(CMD_GET_EEPROM, a, n as u8))?;
            out.extend_from_slice(payload(&r, n).ok_or("Incomplete reply")?);
        }
        Ok(out)
    }

    /// Writes to the EEPROM, keeping the old bytes for undo, and verifies by reading back.
    pub fn write(&self, addr: u16, data: &[u8]) -> Result<(), String> {
        let old = self.read(addr, data.len())?;
        if old == data {
            return Ok(());
        }
        backup::record(addr, &old);
        self.write_raw(addr, data)
    }

    /// Writes without recording history (used by undo).
    pub fn write_raw(&self, addr: u16, data: &[u8]) -> Result<(), String> {
        for (i, part) in data.chunks(CHUNK).enumerate() {
            self.exchange(write_eeprom(addr + (i * CHUNK) as u16, part))?;
        }
        if self.read(addr, data.len())? != data {
            return Err("The mouse did not keep the value (read-back after writing did not match)".into());
        }
        Ok(())
    }

    /// Reads a block, lets `edit` change only what it needs and writes the whole block back.
    pub fn modify(&self, addr: u16, len: usize, edit: impl FnOnce(&mut [u8])) -> Result<(), String> {
        let mut b = self.read(addr, len)?;
        edit(&mut b);
        self.write(addr, &b)
    }
}
