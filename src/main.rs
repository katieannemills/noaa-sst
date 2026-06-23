use netcdf;
use chrono::Utc;
use chrono::TimeZone;
use chrono::Duration;
use chrono::Datelike;
use chrono::Timelike;
use std::env;
use std::error::Error;
use mongodb::{Client, options::{ClientOptions, ResolverConfig}};
use tokio;
use mongodb::bson::{doc};
use serde::{Deserialize, Serialize};
use mongodb::bson::DateTime;

fn tidylon(longitude: f64) -> f64{
    // map longitude on [0,360] to [-180,180], required for mongo indexing
    if longitude <= 180.0{
        return longitude;
    }
    else{
        return longitude-360.0;
    }
}

fn nowstring() -> String{
    // returns a String representing the current ISO8601 datetime

    let now = Utc::now();
    return format!("{}-{:02}-{:02}T{:02}:{:02}:{:02}Z", now.year(), now.month(), now.day(), now.hour(), now.minute(), now.second());
}

// impementing a foreign trait on a forein struct //////////
// per the advice in https://stackoverflow.com/questions/76277096/deconstructing-enums-in-rust/76277117#76277117

struct Wrapper{
    s: String
}

impl std::convert::TryFrom<netcdf::attribute::AttrValue> for Wrapper {
    type Error = &'static str;

    fn try_from(value: netcdf::attribute::AttrValue) -> Result<Self, Self::Error> {

        if let netcdf::attribute::AttrValue::Str(v) = value {
            Ok(Wrapper{s: String::from(v)} )
        } else {
            Err("nope")
        }
    }
}
////////////////////

fn find_basin(basins: &netcdf::Variable, longitude: f64, latitude: f64) -> i32 {    
    let lonplus = (longitude-0.5).ceil()+0.5;
    let lonminus = (longitude-0.5).floor()+0.5;
    let latplus = (latitude-0.5).ceil()+0.5;
    let latminus = (latitude-0.5).floor()+0.5;

    let lonplus_idx = (lonplus - -179.5) as usize;
    let lonminus_idx = (lonminus - -179.5) as usize;
    let latplus_idx = (latplus - -77.5) as usize;
    let latminus_idx = (latminus - -77.5) as usize;

    let corners_idx = [
        // bottom left corner, clockwise
        [latminus_idx, lonminus_idx],
        [latplus_idx, lonminus_idx],
        [latplus_idx, lonplus_idx],
        [latminus_idx, lonplus_idx]
    ];

    let distances = [
        (f64::powi(longitude-lonminus, 2) + f64::powi(latitude-latminus, 2)).sqrt(),
        (f64::powi(longitude-lonminus, 2) + f64::powi(latitude-latplus, 2)).sqrt(),
        (f64::powi(longitude-lonplus, 2) + f64::powi(latitude-latplus, 2)).sqrt(),
        (f64::powi(longitude-lonplus, 2) + f64::powi(latitude-latminus, 2)).sqrt()
    ];

    let mut closecorner_idx = corners_idx[0];
    let mut closedist = distances[0];
    for i in 1..4 {
        if distances[i] < closedist{
            closecorner_idx = corners_idx[i];
            closedist = distances[i];
        }
    }

    match basins.value::<i32,_>(closecorner_idx){
        Ok(idx) => idx as i32,
        Err(e) => panic!("basin problems: {:?} {:#?}", e, closecorner_idx)
    }   
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {

    // mongodb setup ////////////////////////////////////////////////////////////
    // Load the MongoDB connection string from an environment variable:
    let client_uri =
       env::var("MONGODB_URI").expect("You must set the MONGODB_URI environment var!"); 

    // A Client is needed to connect to MongoDB:
    // An extra line of code to work around a DNS issue on Windows:
    let options =
       ClientOptions::parse_with_resolver_config(&client_uri, ResolverConfig::cloudflare())
          .await?;
    let client = Client::with_options(options)?; 

    // // collection objects
    let noaasst = client.database("argo").collection("noaaOIsst");
    //let sst_meta = client.database("argo").collection("timeseriesMeta");
    //let summaries = client.database("argo").collection("summaries");

    // Rust structs to serialize time properly
    #[derive(Serialize, Deserialize, Debug)]
    struct Sourcedoc {
        source: Vec<String>,
        url: String
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct Lattice {
        center: Vec<f64>,
        spacing: Vec<f64>,
        minLat: f64,
        minLon: f64,
        maxLat: f64,
        maxLon: f64
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct SstMetadoc {
        _id: String,
        data_type: String,
        data_info: (Vec<String>, Vec<String>, Vec<Vec<String>>),
        date_updated_argovis: DateTime,
        timeseries: Vec<DateTime>,
        source: Vec<Sourcedoc>,
        lattice: Lattice
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct summaryDoc {
        _id: String,
        data: Vec<String>,
        longitude_grid_spacing_degrees: f64,
        latitude_grid_spacing_degrees: f64,
        longitude_center: f64,
        latitude_center: f64
    }

    /////////////////////////////////////////////////////////////////////////////////

    let file = netcdf::open("/data/sst.week.mean.nc")?;

    // basin lookup
    let basinfile = netcdf::open("/data/basinmask_01.nc")?;
    let basins = &basinfile.variable("BASIN_TAG").expect("Could not find variable 'BASIN_TAG'");

    // all times recorded as days since Jan 1 1800
    let t0 = Utc.with_ymd_and_hms(1800, 1, 1, 0, 0, 0).unwrap();

    // variable extraction
    let lat = &file.variable("lat").expect("Could not find variable 'lat'");
    let lon = &file.variable("lon").expect("Could not find variable 'lon'");
    let sst = &file.variable("sst").expect("Could not find variable 'sst'");
    let time = &file.variable("time").expect("Could not find variable 'time'");

    // construct metadata
    let mut timeseries = Vec::new();
    for timeidx in 0..2326 {
        timeseries.push(bson::DateTime::parse_rfc3339_str((t0 + Duration::days(time.value::<i64, _>(timeidx)?)).to_rfc3339().replace("+00:00", "Z")).unwrap());
    }

    let metadata = SstMetadoc{
        _id: String::from("noaa-oi-sst-v2-high-res"),
        data_type: String::from("noaa-oi-sst-v2-high-res"),
        data_info: (
            vec!(String::from("sst")),
            vec!(String::from("units"), String::from("long_name")),
            vec!(
                vec!(Wrapper::try_from(sst.attribute("units").unwrap().value().unwrap()).unwrap().s,Wrapper::try_from(sst.attribute("long_name").unwrap().value().unwrap()).unwrap().s)
            )
        ),
        date_updated_argovis: bson::DateTime::parse_rfc3339_str(nowstring()).unwrap(),
        timeseries: timeseries,
        source: vec!(
            Sourcedoc{
                source: vec!(String::from("NOAA Optimum Interpolation SST V2 High Resolution")),
                url: String::from("https://psl.noaa.gov/data/gridded/data.noaa.oisst.v2.highres.html")
            }
        ),
        lattice: Lattice{
            center: vec![0.125, 0.125],
            spacing: vec![0.25, 0.25],
            minLat : -89.875,
            minLon : -179.875,
            maxLat : 89.875,
            maxLon : 179.875
        }
    };
    let metadata_doc = bson::to_document(&metadata).unwrap();
    //sst_meta.insert_one(metadata_doc.clone(), None).await?;

    // construct summary doc
    let summary = summaryDoc {
        _id: String::from("noaasstsummary"),
        data: vec!(String::from("sst")),
        longitude_grid_spacing_degrees: 0.25,
        latitude_grid_spacing_degrees: 0.25,
        longitude_center: 0.125,
        latitude_center: 0.125
    };
    let summary_doc = bson::to_document(&summary).unwrap();
    //summaries.insert_one(summary_doc.clone(), None).await?;

    // construct data docs
    for latidx in 288..720 {
        let lat = lat.value::<f64, _>([latidx])?;
        let mut docs = Vec::new(); // collect all the docs for this latitude, and write all at once.
        for lonidx in 0..1440 {
            let lon = tidylon(lon.value::<f64, _>([lonidx])?);
            let mut ssts = Vec::new();
            for timeidx in 0..2326 {
                ssts.push(sst.value::<f64, _>([timeidx, latidx, lonidx])?);
            }
            if ssts.iter().all(|&x| x == -9.969209968386869e+36){
                continue; // all fill values, drop it
            }
            let basin = find_basin(&basins, lon, lat);
            let id = [lon.to_string(), lat.to_string()].join("_");
            let data = doc!{
                "_id": id,
                "metadata": ["noaa-oi-sst-v2-high-res"],
                "basin": basin,
                "geolocation": {
                    "type": "Point",
                    "coordinates": [lon, lat]
                },
                "data": [ssts.clone()],
                "level": 0.0
            };
            docs.push(data);
        }
        if !docs.is_empty(){
            noaasst.insert_many(docs, None).await?;
        }
        println!("wrote lat index {}", latidx);
    }

    Ok(())
}
