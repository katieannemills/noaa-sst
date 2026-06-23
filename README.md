This repo contains scripts and data for populating NOAA SST information from https://psl.noaa.gov/data/gridded/data.noaa.oisst.v2.html in Argovis.

## Rebuilding from scratch
 
 - Download the high res data ala `wget https://downloads.psl.noaa.gov/Datasets/noaa.oisst.v2.highres/sst.week.mean.nc`, last accessed 2026/04/10.
 - Place the raw data in `/tmp`, along with `basinmask_01.nc` 
 - Build container with `docker image build -t argovis/sst:dev .`
 - Rebuild collection with something to the tune of:

```
docker container run -d --network argovis -v /tmp:/data:z --env MONGODB_URI=mongodb://database/argo argovis/sst:dev /app/target/release/sst
```

 - Add indexes to the `argo:noaaOIsst` collection: `{'geolocation':'2dsphere'}` and `{'level':1, 'geolocation':'2dsphere'}`
