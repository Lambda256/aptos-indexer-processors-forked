use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};

const DEFAULT_MESSAGE_TIMEOUT_MS: &str = "5000";
const DEFAULT_QUEUE_BUFFERING_MAX_MS: &str = "2000";

pub trait CustomProducer {
    fn new(brokers: &str) -> Self;
    async fn send_to_mq<'a, T>(&'a self, topic: &'a str, items: &'a [T]) -> Result<(), String>
    where
        T: serde::Serialize + 'a;
}

#[derive(Clone)]
pub enum CustomProducerEnum {
    Kafka(KafkaProducer),
    Noop(NoopProducer),
}

impl CustomProducer for CustomProducerEnum {
    fn new(brokers: &str) -> CustomProducerEnum {
        if brokers.is_empty() {
            CustomProducerEnum::Noop(NoopProducer::new(brokers))
        } else {
            CustomProducerEnum::Kafka(KafkaProducer::new(brokers))
        }
    }

    async fn send_to_mq<'a, T>(&'a self, topic: &'a str, items: &'a [T]) -> Result<(), String>
    where
        T: serde::Serialize + 'a,
    {
        match self {
            CustomProducerEnum::Kafka(p) => p.send_to_mq(topic, items).await,
            CustomProducerEnum::Noop(p) => p.send_to_mq(topic, items).await,
        }
    }
}

#[derive(Clone)]
pub struct KafkaProducer {
    producer: FutureProducer,
}

impl CustomProducer for KafkaProducer {
    fn new(brokers: &str) -> KafkaProducer {
        let producer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", DEFAULT_MESSAGE_TIMEOUT_MS)
            .set("queue.buffering.max.ms", DEFAULT_QUEUE_BUFFERING_MAX_MS)
            .set("compression.codec", "snappy")
            .set("request.required.acks", "all")
            .set("message.max.bytes", "10000000")
            .set("batch.size", "50000000")
            .create()
            .expect("Producer creation error");
        KafkaProducer { producer }
    }

    async fn send_to_mq<'a, T>(&'a self, topic: &'a str, items: &'a [T]) -> Result<(), String>
    where
        T: serde::Serialize + 'a,
    {
        let futures = items
            .iter()
            .map(|item| {
                let payload = serde_json::to_string(&item).unwrap();
                let record = FutureRecord::to(topic).key(&()).payload(&payload);
                let fut = self.producer.send_result(record).unwrap();
                tokio::spawn(async move {
                    let delivery_status = fut.await;
                    delivery_status
                })
            })
            .collect::<Vec<_>>();

        for fut in futures {
            let res = fut.await;
            if res.is_err() {
                return Err(format!("Error: {:?}", res.unwrap_err()));
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct NoopProducer;

impl CustomProducer for NoopProducer {
    fn new(_brokers: &str) -> NoopProducer {
        NoopProducer
    }

    async fn send_to_mq<'a, T>(&'a self, _topic: &'a str, _items: &'a [T]) -> Result<(), String>
    where
        T: serde::Serialize + 'a,
    {
        Ok(())
    }
}
