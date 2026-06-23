# usage: python proofread.py
# expects /tmp/sst.week.mean.nc
import xarray, random, time, math
from pymongo import MongoClient

def tidylon(longitude):
    # map longitude on [0,360] to [-180,180], required for mongo indexing
    if longitude <= 180.0:
        return longitude;
    else:
        return longitude-360.0;

# db connection
client = MongoClient('mongodb://database/argo')
db = client.argo

# data files
upstream = xarray.open_dataset('/tmp/sst.week.mean.nc', decode_times=False)

# metadata record
metadata = list(db.timeseriesMeta.find({"_id":'noaa-oi-sst-v2-high-res'}))[0]

while True:

        latidx = math.floor(720*random.random())
        lonidx = math.floor(1440*random.random())
        timeidx = math.floor(2326*random.random())
        id = str(tidylon(upstream['lon'][lonidx].to_dict()['data'])) + "_" + str(upstream['lat'][latidx].to_dict()['data'])
        print(id)
        truth = upstream['sst'][timeidx, latidx, lonidx].to_dict()['data']
        if not math.isnan(truth): # nans are on land
            data = list(db.noaaOIsst.find({"_id": id }))[0]
            if round(data['data'][0][timeidx],2) != round(truth, 2):
                print('mismatch:', latidx, lonidx, timeidx, data['data'][0][timeidx], upstream['sst'][timeidx, latidx, lonidx].to_dict()['data'])
            else:
                print('ok')

        time.sleep(1)
