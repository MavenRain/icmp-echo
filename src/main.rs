use {
    derive_more::{From, Into, TryInto},
    futures_util::{stream::iter, FutureExt, StreamExt, TryFutureExt, TryStreamExt},
    icmp_socket::{
        packet::{IcmpPacketBuildError, WithEchoRequest},
        IcmpSocket, IcmpSocket4, Icmpv4Message, Icmpv4Packet,
    },
    std::{
        net::{AddrParseError, Ipv4Addr},
        num::ParseIntError,
        str::FromStr,
        time::{Duration, Instant},
    },
    structopt::StructOpt,
};

#[derive(Debug, From, Into)]
struct RequestsToSend(u16);

impl<'a> TryFrom<&'a str> for RequestsToSend {
    type Error = Error;
    fn try_from(text: &'a str) -> Result<Self, Self::Error> {
        match u16::from_str(text)? {
            x if x > 10 => Err("only ten or less requests are supported".to_string().into()),
            x if x == 0 => Err("at least one ping must be requested".to_string().into()),
            x => Ok(x.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, From, Into)]
struct TransmissionInterval(u16);

impl<'a> TryFrom<&'a str> for TransmissionInterval {
    type Error = Error;
    fn try_from(text: &'a str) -> Result<Self, Self::Error> {
        match u16::from_str(text)? {
            x if x > 1000 => Err("only one second or less intervals supported"
                .to_string()
                .into()),
            x if x == 0 => Err("zero interval is not supported".to_string().into()),
            x => Ok(x.into()),
        }
    }
}

#[derive(Debug)]
struct Arg {
    destination: Ipv4Addr,
    requests: RequestsToSend,
    interval: TransmissionInterval,
}

impl From<Options> for (Ipv4Addr, RequestsToSend, TransmissionInterval) {
    fn from(options: Options) -> Self {
        let arg = options.arg;
        (arg.destination, arg.requests, arg.interval)
    }
}

impl From<(Ipv4Addr, RequestsToSend, TransmissionInterval)> for Arg {
    fn from(
        (destination, requests, interval): (Ipv4Addr, RequestsToSend, TransmissionInterval),
    ) -> Self {
        Self {
            destination,
            requests,
            interval,
        }
    }
}

#[derive(Debug, strum_macros::Display, From, TryInto)]
enum Error {
    AddressParsing(AddrParseError),
    Io(std::io::Error),
    NumberParsing(ParseIntError),
    PacketBuilding(IcmpPacketBuildError),
    Uncategorized(String),
}

fn parse_arg(arg: &str) -> Result<Arg, Error> {
    let mut comma_separated_values = arg.split(",").take(3);
    let destination = comma_separated_values.next();
    let requests = comma_separated_values.next();
    let interval = comma_separated_values.next();
    let (destination, requests, interval) =
        destination.and_then(|destination| requests.and_then(|requests|
            interval.map(|interval| (destination, requests, interval))
        )).ok_or_else(|| "Usage of ICMP Ping requires an argument consisting of a comma-delimited list of IP address, number of requests, and ping interval".to_string())?;
    let destination = destination.parse::<Ipv4Addr>()?;
    let requests: RequestsToSend = requests.try_into()?;
    let interval: TransmissionInterval = interval.try_into()?;
    Ok((destination, requests, interval).into())
}

#[derive(Debug, From, Into, StructOpt)]
struct Options {
    #[structopt(parse(try_from_str = parse_arg))]
    arg: Arg,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let (address, requests, interval): (Ipv4Addr, RequestsToSend, TransmissionInterval) =
        Options::from_args().into();
    let socket: IcmpSocket4 = "0.0.0.0".parse::<Ipv4Addr>()?.try_into()?;
    iter(0..requests.into())
        .map(Ok)
        .try_fold(socket, |mut socket, sequence| {
            tokio::time::sleep(Duration::from_millis(u16::from(interval).into()))
                .then(move |_| async move {
                    Icmpv4Packet::with_echo_request(5091, sequence, "test packet".as_bytes().to_vec())
                        .map(|packet| {
                            socket.set_timeout(Some(Duration::from_secs(5)));
                            socket.send_to(address, packet)
                        })
                        .map(|_| (socket, Instant::now()))
                })
                .and_then(|(mut socket, send_time)| async move {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(5)) => Ok(socket),
                        Ok((Icmpv4Packet {
                            code: _,
                            typ: _,
                            checksum: _,
                            message: Icmpv4Message::EchoReply {
                                identifier: _,
                                sequence,
                                payload: _
                            }
                        }, address)) = async { socket.rcv_from() } => {
                            let elapsed = Instant::now() - send_time;
                            let address = address.as_socket_ipv4().map(|sock| sock.ip().clone().to_string()).unwrap_or_default();
                            println!("{},{:?},{:?}", address, sequence, elapsed.as_micros());
                            Ok(socket)
                        }
                    }
                })
        })
        .await
        .map(|_| ())
        .map_err(Into::into)
}
