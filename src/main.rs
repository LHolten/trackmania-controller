use std::{
    fs::File,
    io::{Read, Write},
    net::TcpStream,
};

use dxr::{Fault, FaultResponse, MethodCall, MethodResponse, TryFromValue};
use rand::random;

struct Client {
    client: TcpStream,
    handle: u32,
}

impl Client {
    pub fn new() -> Self {
        let mut client = Client {
            client: TcpStream::connect("localhost:5000").unwrap(),
            handle: 0x80000000,
        };
        let len = client.read_u32();
        let hello = client.read_msg(len);
        assert_eq!(hello, "GBXRemote 2");

        let suc: bool = client.call("SetApiVersion", "2023-04-24").unwrap();
        assert!(suc);
        let suc: bool = client
            .call("Authenticate", ["SuperAdmin", "SuperAdmin"])
            .unwrap();
        assert!(suc);

        client
    }

    pub fn read_u32(&mut self) -> u32 {
        let mut val = [0; 4];
        self.client.read_exact(&mut val).unwrap();
        u32::from_le_bytes(val)
    }

    pub fn write_u32(&mut self, val: u32) {
        self.client.write_all(&val.to_le_bytes()).unwrap();
    }

    pub fn new_handle(&mut self) -> u32 {
        self.handle += 1;
        if self.handle >= 0xffffff00 {
            self.handle = 0x80000000;
        }
        self.handle
    }

    pub fn read_msg(&mut self, len: u32) -> String {
        let mut msg = vec![0; len as usize];
        self.client.read_exact(&mut msg).unwrap();
        String::from_utf8(msg).unwrap()
    }

    pub fn call<R>(&mut self, f: &'static str, args: impl dxr::TryToParams) -> Result<R, Fault>
    where
        R: TryFromValue,
    {
        let method = MethodCall::new(f.to_owned(), args.try_to_params().unwrap());
        let msg = dxr::serialize_xml(&method).unwrap();
        self.write_u32(msg.len() as u32);
        let handle = self.new_handle();
        self.write_u32(handle);
        self.client.write_all(msg.as_bytes()).unwrap();

        let len = self.read_u32();
        let new_handle = self.read_u32();
        assert_eq!(handle, new_handle);
        let msg = self.read_msg(len);
        if let Ok(res) = dxr::deserialize_xml::<FaultResponse>(&msg) {
            let fault = Fault::try_from(res).unwrap();
            return Err(fault);
        }
        let res: MethodResponse = dxr::deserialize_xml(&msg).unwrap();
        Ok(R::try_from_value(&res.inner()).unwrap())
    }

    fn download_map(&mut self, id: &str) {
        let dir: String = self.call("GetMapsDirectory", ()).unwrap();

        if let Ok(mut file) = File::create_new(format!("{dir}{id}.Map.Gbx")) {
            let url = format!("https://trackmania.exchange/maps/download/{id}");
            // trackmania.exchange does not like it if we don't give a user_agent
            let req = reqwest::blocking::Client::builder()
                .user_agent("hytak-server-util")
                .build()
                .unwrap();
            req.get(url).send().unwrap().copy_to(&mut file).unwrap();
        } else {
            println!("map is already downloaded")
        }

        let rel_path = format!("{id}.Map.Gbx");
        // let next: MapInfo = self.call("GetNextMapInfo", ());

        if let Err(err) = self.call::<bool>("InsertMap", rel_path.as_str()) {
            println!("while inserting map: {}", err.string())
        }
        // self.call::<bool>("ChooseNextMap", rel_path.as_str())
        //     .unwrap();
        // self.call::<bool>("NextMap", ()).unwrap();
    }
}

fn main() {
    // let arg: Vec<String> = std::env::args().collect();

    let mut client = Client::new();

    // client.download_map(&arg[1]);

    for _ in 0..20 {
        let id = random::<u32>() % 100000;
        client.download_map(&format!("{id}"));
    }
    // client.call::<bool>("NextMap", ()).unwrap();
}

#[allow(non_snake_case, dead_code)]
#[derive(TryFromValue, Debug)]
struct MapInfo {
    Name: String,
    UId: String,
    FileName: String,
    Environnement: String,
    Author: String,
    AuthorNickname: String,
    GoldTime: i32,
    CopperPrice: i32,
    MapType: String,
    MapStyle: String,
}
