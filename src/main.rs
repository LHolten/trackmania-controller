use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    net::TcpStream,
};

use dxr::{Fault, FaultResponse, MethodCall, MethodResponse, TryFromValue};

use color_eyre::eyre::ContextCompat;

struct Client {
    client: TcpStream,
    exchange: reqwest::blocking::Client,
    handle: u32,
    msgs: HashMap<u32, Option<String>>,
}

impl Client {
    pub fn new() -> Self {
        let mut client = Client {
            client: TcpStream::connect("localhost:5000").unwrap(),
            // trackmania.exchange does not like it if we don't give a user_agent
            exchange: reqwest::blocking::Client::builder()
                .user_agent("hytak-server-util")
                .build()
                .unwrap(),
            handle: 0x80000000,
            msgs: HashMap::new(),
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

        let suc: bool = client.call("EnableCallbacks", [true]).unwrap();
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

        let msg = loop {
            self.msgs.insert(handle, None);
            self.await_messages();

            if let Some(msg) = self.msgs.remove(&handle).unwrap() {
                break msg;
            }
        };

        if let Ok(res) = dxr::deserialize_xml::<FaultResponse>(&msg) {
            let fault = Fault::try_from(res).unwrap();
            return Err(fault);
        }
        let res: MethodResponse = dxr::deserialize_xml(&msg).unwrap();
        Ok(R::try_from_value(&res.inner()).unwrap())
    }

    /// this will wait for callbacks or response for one of `self.msgs`
    pub fn await_messages(&mut self) {
        loop {
            let len = self.read_u32();
            let handle = self.read_u32();
            let msg = self.read_msg(len);

            // were we expecting a response for this handle?
            if self.msgs.remove(&handle).is_some() {
                self.msgs.insert(handle, Some(msg));
                return;
            }

            self.handle_callback(&msg, handle);
        }
    }

    pub fn handle_callback(&mut self, msg: &str, _handle: u32) {
        let call: MethodCall = dxr::deserialize_xml(msg).unwrap();

        if call.name() == "ManiaPlanet.BeginMap" {
            let random_id = self.random_map_id().unwrap();
            println!("downloading map {random_id}");
            self.download_map(random_id);
        }

        // println!("{call:?}")
    }

    fn random_map_id(&mut self) -> color_eyre::Result<u64> {
        let res = self
            .exchange
            .get("http://trackmania.exchange/mapsearch2/search?api=on&random=1&etags=23,37,40&mtype=TM_Race")
            .send()?;

        let val: serde_json::Value = serde_json::from_str(&res.text()?)?;
        let id = val
            .get("results")
            .context("no results")?
            .get(0)
            .context("no results")?
            .get("TrackID")
            .context("no track id")?
            .as_u64()
            .context("not a number")?;

        Ok(id)
    }

    fn download_map(&mut self, id: u64) {
        let dir: String = self.call("GetMapsDirectory", ()).unwrap();

        if let Ok(mut file) = File::create_new(format!("{dir}{id}.Map.Gbx")) {
            let url = format!("https://trackmania.exchange/maps/download/{id}");
            let req = self.exchange.get(url);
            req.send().unwrap().copy_to(&mut file).unwrap();
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

    // for _ in 0..20 {
    //     let random_id = client.random_map_id().unwrap();
    //     client.download_map(random_id);
    // }
    client.call::<bool>("NextMap", ()).unwrap();
    client.await_messages();
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
