const { faker } = require("@faker-js/faker");
const axios = require("axios");

faker.seed(42);

const auth = "cm9vdEBleGFtcGxlLmNvbTpDb21wbGV4cGFzcyMxMjM=";

const logCount = 100000;
const stride = 5000;

const port1 = 5080;
const port2 = 5090;
const port3 = 5070;

let tsStart = 1744788769499000;

let namespaces = [];
for (let i = 0;i<75;i++){
  namespaces.push(faker.git.branch());
}

let pod_names = []
for(let i=0;i<150;i++){
  pod_names.push(faker.hacker.adjective())
}

function getLog() {
  const ts = faker.date.past();
  const l1 = faker.string.hexadecimal({ length: 10 });
  const l2 = faker.string.numeric(10);
  const l3 = faker.lorem.words(5);
  const l4 = faker.system.filePath();
  const l5 = faker.system.commonFileName();
  const l6 = faker.word.words(5);
  let namespace = namespaces[Math.floor((Math.random() * 100)) % namespaces.length];
  let pod_name = pod_names[Math.floor((Math.random() * 100)) % pod_names.length];

  //let tsEnabled = Math.random() > 0.1;
  let tsEnabled = true; 
  let diff = Math.random()>0.5? 1:-1;
  let _timestamp = tsStart + diff*(Math.floor(Math.random()*48*3600)*1000)*1000;

  return {
    _timestamp: tsEnabled?_timestamp:undefined,
    id: faker.string.uuid(),
    kubernetes_namespace:namespace,
    kubernetes_pod_name:pod_name,
    log: `[${ts}] ${l1} ${l2} ${l3} ${l4}/${l5} ${l6}`,
  };
}

async function send(data){
    for(const port of [port1]){
      let start = Date.now();
      let resp = await axios.post(
        `http://localhost:${port}/api/default/test/_json`,
        data,
        {
          headers: {
            Authorization: "Basic " + auth,
          },
        },
      );
      let end = Date.now();
      console.log(`port ${port} time : `+(end-start));
    }

}

async function main() {
  for (let i = 0; i < logCount; i += stride) {
    let t = [];
    console.log(`i = ${i}`);
    for (let j = 0; j < stride; j++) {
      t.push(getLog());
    }
    await send(t);
  }
}

main();