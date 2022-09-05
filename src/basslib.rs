#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::upper_case_acronyms)]

use std::{str::FromStr, time::Duration};

use libc::{c_char, c_void};
use widestring::U16CString;
// use widestring::U16CString;

macro_rules! bce {
    ($e:expr) => {{
        let res = unsafe { $e };
        let er = unsafe { BASS_ErrorGetCode() };
        if er != 0 {
            panic!("bass error {}", er);
        }
        res
    }};
}

use crate::stream::TrackMetadata;

type BOOL = i32;
type QWORD = u64;
type DWORD = u32;
type HSTREAM = DWORD;

pub static BASS_UNICODE: u32 = 0x80000000;
pub static BASS_SAMPLE_FLOAT: u32 = 256;
pub static BASS_POS_BYTE: u32 = 0;

#[link(name = "bassflac")]
extern "C" {
    fn BASS_FLAC_StreamCreateFile(
        memory: BOOL,
        file: *const c_void,
        offset: QWORD,
        length: QWORD,
        flags: DWORD,
    ) -> HSTREAM;
}

pub fn BFStreamCreateFile(path: String) -> HSTREAM {
    let s = U16CString::from_str(path).unwrap();
    bce!(BASS_FLAC_StreamCreateFile(
        0,
        s.into_raw() as _,
        0,
        0,
        BASS_UNICODE | BASS_SAMPLE_FLOAT
    ))
}

#[link(name = "bass")]
extern "C" {
    fn BASS_Init(
        device: i32,
        freq: DWORD,
        flags: DWORD,
        win: *mut c_void,
        clsid: *mut c_void,
    ) -> BOOL;
    fn BASS_Start() -> BOOL;
    fn BASS_ChannelPlay(handel: HSTREAM, restart: BOOL) -> BOOL;
    fn BASS_ChannelIsActive(_: HSTREAM) -> DWORD;
    fn BASS_ChannelFree(_: HSTREAM) -> BOOL;
    fn BASS_ErrorGetCode() -> i32;
    fn BASS_ChannelPause(handle: DWORD) -> BOOL;
    fn BASS_ChannelStop(handle: DWORD) -> BOOL;
    fn BASS_ChannelGetPosition(handle: DWORD, mode: DWORD) -> QWORD;
    fn BASS_ChannelBytes2Seconds(handle: DWORD, pos: QWORD) -> f64;
    fn BASS_ChannelSetPosition(handle: DWORD, pos: QWORD, mode: DWORD) -> BOOL;
    fn BASS_ChannelSeconds2Bytes(handle: DWORD, pos: f64) -> QWORD;
    fn BASS_GetDeviceInfo(device: DWORD, info: *mut BASS_DEVICEINFO) -> BOOL;
    fn BASS_ChannelGetLength(handle: DWORD, mode: DWORD) -> QWORD;
    // fn BASS_SetVolume(volume: f32) -> BOOL;
    fn BASS_Pause() -> BOOL;
    fn BASS_ChannelSetAttribute(handle: DWORD, attrib: DWORD, value: f32) -> BOOL;
    fn BASS_SetConfig(option: DWORD, value: BOOL) -> DWORD;
    fn BASS_Free() -> BOOL;
    fn BASS_StreamCreateFile(
        memory: BOOL,
        file: *const c_void,
        offset: QWORD,
        length: QWORD,
        flags: DWORD,
    ) -> HSTREAM;
}

pub fn BStreamCreateFile(path: String) -> HSTREAM {
    let s = U16CString::from_str(path).unwrap();
    bce!(BASS_StreamCreateFile(
        0,
        s.into_raw() as _,
        0,
        0,
        BASS_UNICODE | BASS_SAMPLE_FLOAT
    ))
}

pub fn BFree() -> bool {
    bce!(BASS_Free()) != 0
}

pub fn BSetConfig(option: u32, value: BOOL) -> u32 {
    bce!(BASS_SetConfig(option, value))
}

pub fn BChannelStop(stream: &MediaStream) -> bool {
    bce!(BASS_ChannelStop(stream.handle)) != 0
}

pub fn BPause() -> bool {
    bce!(BASS_Pause()) != 0
}

pub fn BChannelPause(stream: &MediaStream) -> bool {
    if matches!(BChannelIsActive(stream), channel_state::BASS_ACTIVE_PLAYING) {
        bce!(BASS_ChannelPause(stream.handle)) != 0
    } else {
        false
    }
}

pub static BASS_ATTRIB_VOL: u32 = 2;

pub fn BSetVolume(stream: &MediaStream, volume: f32) -> bool {
    bce!(BASS_ChannelSetAttribute(
        stream.handle,
        BASS_ATTRIB_VOL,
        volume
    )) != 0
}

pub fn BChannelGetLength(stream: &MediaStream, mode: u32) -> u64 {
    bce!(BASS_ChannelGetLength(stream.handle, mode))
}

pub fn BInit(device: i32, freq: u32, flags: u32, win: u32) -> bool {
    bce!(BASS_Init(device, freq, flags, win as _, 0 as _)) != 0
}

pub fn BStart() -> bool {
    bce!(BASS_Start()) != 0
}

pub fn BChannelPlay(stream: &MediaStream, restart: BOOL) -> bool {
    bce!(BASS_ChannelPlay(stream.handle, restart)) != 0
}

pub enum channel_state {
    BASS_ACTIVE_STOPPED,
    BASS_ACTIVE_PLAYING,
    BASS_ACTIVE_PAUSED,
    BASS_ACTIVE_PAUSED_DEVICE,
    BASS_ACTIVE_STALLED,
}

pub fn BChannelIsActive(stream: &MediaStream) -> channel_state {
    match bce!(BASS_ChannelIsActive(stream.handle)) {
        0 => channel_state::BASS_ACTIVE_STOPPED,
        1 => channel_state::BASS_ACTIVE_PLAYING,
        2 => channel_state::BASS_ACTIVE_PAUSED,
        3 => channel_state::BASS_ACTIVE_PAUSED_DEVICE,
        4 => channel_state::BASS_ACTIVE_STALLED,
        _ => panic!("unknown state"),
    }
}

pub fn BChannelGetPosition(stream: &MediaStream, mode: u32) -> u64 {
    bce!(BASS_ChannelGetPosition(stream.handle, mode))
}

pub fn BChannelBytes2Seconds(stream: &MediaStream, pos: u64) -> f64 {
    bce!(BASS_ChannelBytes2Seconds(stream.handle, pos))
}

pub fn BChannelSetPosition(stream: &MediaStream, pos: u64, mode: u32) -> bool {
    bce!(BASS_ChannelSetPosition(stream.handle, pos, mode)) != 0
}
pub fn BChannelSeconds2Bytes(stream: &MediaStream, pos: f64) -> u64 {
    bce!(BASS_ChannelSeconds2Bytes(stream.handle, pos))
}
pub fn BGetDeviceInfo(device: u32) -> BASS_DEVICEINFO {
    unsafe {
        let info: *mut BASS_DEVICEINFO =
            libc::malloc(std::mem::size_of::<BASS_DEVICEINFO>() as libc::size_t)
                as *mut BASS_DEVICEINFO;
        BASS_GetDeviceInfo(device, info);
        // println!("we are here!");
        // // let name = U16CString::from_ptr_str((*info).name as _);
        // let mut shift = 0;
        // let mut svec = vec![];
        // while *((*info).name.offset(shift)) != 0 {
        //     svec.push(*((*info).name.offset(shift)) as u8);
        //     shift += 1;
        //     // println!("{:?}", *((*info).name.offset(shift)));
        // }
        // let name = std::str::from_utf8(&svec).unwrap().to_owned();
        // println!("{:?}", name);
        let ret = (*info).clone();
        libc::free(info as *mut libc::c_void);
        ret
    }
}

pub struct MediaStream {
    pub handle: HSTREAM,
    pub metadata: TrackMetadata,
}

impl Drop for MediaStream {
    fn drop(&mut self) {
        bce!(BASS_ChannelFree(self.handle));
    }
}

impl MediaStream {
    pub fn seek_to(&self, dur: Duration) {
        // let ct = BChannelBytes2Seconds(self, BChannelGetPosition(self, BASS_POS_BYTE));
        // ct = current time
        BChannelSetPosition(
            self,
            BChannelSeconds2Bytes(
                self,
                (dur.as_secs_f64()).clamp(0.0, self.metadata.full_time_secs.unwrap() as f64),
            ),
            BASS_POS_BYTE,
        );
    }

    pub fn seek_backward(&self, dur: Duration) {
        let ct = BChannelBytes2Seconds(self, BChannelGetPosition(self, BASS_POS_BYTE));
        // ct = current time
        BChannelSetPosition(
            self,
            BChannelSeconds2Bytes(
                self,
                (ct - dur.as_secs_f64()).clamp(0.0, self.metadata.full_time_secs.unwrap() as f64),
            ),
            BASS_POS_BYTE,
        );
    }

    pub fn seek_forward(&self, dur: Duration) {
        let ct = BChannelBytes2Seconds(self, BChannelGetPosition(self, BASS_POS_BYTE));
        // ct = current time
        BChannelSetPosition(
            self,
            BChannelSeconds2Bytes(
                self,
                (ct + dur.as_secs_f64()).clamp(0.0, self.metadata.full_time_secs.unwrap() as f64),
            ),
            BASS_POS_BYTE,
        );
    }

    pub fn as_secs(&self) -> f64 {
        BChannelBytes2Seconds(self, BChannelGetPosition(self, BASS_POS_BYTE))
    }

    pub fn new(path: String) -> Self {
        MediaStream {
            handle: MediaStream::make_handle(path.clone()),
            metadata: TrackMetadata::from_str(path.as_str()).unwrap(),
        }
    }

    pub fn make_handle(path: String) -> HSTREAM {
        let ext = std::path::Path::new(&path)
            .extension()
            .unwrap()
            .to_string_lossy()
            .to_string();
        match ext.as_str() {
            "flac" => BFStreamCreateFile(path),
            "mp3" => BStreamCreateFile(path),
            _ => panic!("i don't support this"),
        }
    }
}

#[repr(C)]
#[derive(Clone)]
pub struct BASS_DEVICEINFO {
    name: *mut c_char,
    driver: *mut c_char,
    flags: DWORD,
}

impl Drop for BASS_DEVICEINFO {
    fn drop(&mut self) {
        unsafe {
            libc::free(self.name as *mut libc::c_void);
            libc::free(self.driver as *mut libc::c_void);
        }
    }
}

pub fn get_device_count() -> u8 {
    unsafe {
        let info: *mut BASS_DEVICEINFO =
            libc::malloc(std::mem::size_of::<BASS_DEVICEINFO>() as libc::size_t)
                as *mut BASS_DEVICEINFO;
        let mut a = 2;
        let mut count = 0;
        while BASS_GetDeviceInfo(a, info) != 0 {
            if (*info).flags & 1 != 0 {
                count += 1;
            }
            a += 1;
        }
        // println!("{count}");
        libc::free(info as *mut libc::c_void);
        count
    }
}

pub fn get_device_name(n: u32) -> String {
    unsafe {
        let info: *mut BASS_DEVICEINFO =
            libc::malloc(std::mem::size_of::<BASS_DEVICEINFO>() as libc::size_t)
                as *mut BASS_DEVICEINFO;
        BASS_GetDeviceInfo(n, info);
        let mut shift = 0;
        let mut svec = vec![];
        while *((*info).name.offset(shift)) != 0 {
            svec.push(*((*info).name.offset(shift)) as u8);
            shift += 1;
            // println!("{:?}", *((*info).name.offset(shift)));
        }
        let name = std::str::from_utf8(&svec).unwrap().to_owned();
        // println!("{:?}", name);
        libc::free(info as *mut libc::c_void);
        name
    }
}
