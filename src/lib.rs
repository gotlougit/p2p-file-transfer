mod parsing;
mod socket;
mod connection;

#[cfg(test)]
mod test {
    use std::net::SocketAddr;
    use std::str::FromStr;

    use crate::parsing::*;
    use crate::socket::*;
    use crate::connection::MTU;

    #[test]
    fn parsing_test_primitives() {
        let ack = get_primitive(PrimitiveMessage::ACK);
        let nack = get_primitive(PrimitiveMessage::NACK);
        let resend = get_primitive(PrimitiveMessage::RESEND);
        let end = get_primitive(PrimitiveMessage::END);
        assert_eq!(parse_primitive(&ack, ack.len()), PrimitiveMessage::ACK);
        assert_eq!(parse_primitive(&nack, nack.len()), PrimitiveMessage::NACK);
        assert_eq!(
            parse_primitive(&resend, resend.len()),
            PrimitiveMessage::RESEND
        );
        assert_eq!(parse_primitive(&end, end.len()), PrimitiveMessage::END);
    }

    #[test]
    fn parsing_test_send() {
        let fname = String::from("testfilename");
        let auth = String::from("testauthtoken");
        let sendreq = send_req(&fname, &auth);
        assert_eq!(parse_send_req(&sendreq, sendreq.len()).is_some(), true);
        if let Some((parsedfname, parsedauth)) = parse_send_req(&sendreq, sendreq.len()) {
            assert_eq!(parsedfname, fname);
            assert_eq!(parsedauth, auth);
        }
        let ack = get_primitive(PrimitiveMessage::ACK);
        assert_eq!(parse_send_req(&ack, ack.len()).is_none(), true);
    }

    #[test]
    fn parsing_test_filesize() {
        let fsize1 = 0;
        let fsize2 = 100;
        let fsize3 = usize::max_value();

        let packet1 = filesize_packet(fsize1);
        let packet2 = filesize_packet(fsize2);
        let packet3 = filesize_packet(fsize3);

        let result1 = parse_filesize_packet(&packet1, packet1.len());
        let result2 = parse_filesize_packet(&packet2, packet2.len());
        let result3 = parse_filesize_packet(&packet3, packet3.len());

        assert_eq!(result1.is_some(), true);
        assert_eq!(result2.is_some(), true);
        assert_eq!(result3.is_some(), true);

        if let Some(parsedfsize1) = result1 {
            assert_eq!(parsedfsize1, fsize1);
        }
        if let Some(parsedfsize2) = result2 {
            assert_eq!(parsedfsize2, fsize2);
        }
        if let Some(parsedfsize3) = result3 {
            assert_eq!(parsedfsize3, fsize3);
        }
    }

    #[test]
    fn parsing_test_last_recv() {
        let offset1 = 0;
        let offset2 = 100;
        let offset3 = usize::max_value();

        let packet1 = last_received_packet(offset1);
        let packet2 = last_received_packet(offset2);
        let packet3 = last_received_packet(offset3);

        let result1 = parse_last_received(&packet1, packet1.len());
        let result2 = parse_last_received(&packet2, packet2.len());
        let result3 = parse_last_received(&packet3, packet3.len());

        assert_eq!(result1.is_some(), true);
        assert_eq!(result2.is_some(), true);
        assert_eq!(result3.is_some(), true);

        if let Some(parsedoffset1) = result1 {
            assert_eq!(parsedoffset1, offset1);
        }
        if let Some(parsedoffset2) = result2 {
            assert_eq!(parsedoffset2, offset2);
        }
        if let Some(parsedoffset3) = result3 {
            assert_eq!(parsedoffset3, offset3);
        }
    }

    #[test]
    fn parsing_test_data_packet() {
        let offset1 = 0;
        let offset2 = 1000;
        let offset3 = 10000;

        let data1: Vec<u8> = Vec::new();
        let data2: Vec<u8> = Vec::from(*b"Hello world this is data");
        let data3: Vec<u8> = Vec::from([0u8; 10000]);

        let dp1 = data_packet(offset1, &data1);
        let dp2 = data_packet(offset2, &data2);
        let dp3 = data_packet(offset3, &data3);

        let result1 = parse_data_packet(&dp1, dp1.len());
        let result2 = parse_data_packet(&dp2, dp2.len());
        let result3 = parse_data_packet(&dp3, dp3.len());

        assert_eq!(result1.is_some(), true);
        assert_eq!(result2.is_some(), true);
        assert_eq!(result3.is_some(), true);

        if let Some((parsedoffset1, parseddata1)) = result1 {
            assert_eq!(parsedoffset1, offset1);
            assert_eq!(parseddata1, data1);
        }

        if let Some((parsedoffset2, parseddata2)) = result2 {
            assert_eq!(parsedoffset2, offset2);
            assert_eq!(parseddata2, data2);
        }

        if let Some((parsedoffset3, parseddata3)) = result3 {
            assert_eq!(parsedoffset3, offset3);
            assert_eq!(parseddata3, data3);
        }
    }

    #[tokio::test]
    async fn test_dummy_socket() {
        let dum1 = DummySocket {
            send_proper: false,
            recv_proper: false,
        };
        let dum2 = DummySocket {
            send_proper: true,
            recv_proper: true,
        };
        let dum3 = DummySocket {
            send_proper: true,
            recv_proper: false,
        };
        let msg = *b"Hello world";
        let ip1 = SocketAddr::from_str("127.0.0.1:1026").unwrap();
        //test sending
        match dum1.send_to(&msg[..], &ip1).await {
            Ok(size) => {
                assert_eq!(size, 0);
            }
            Err(_) => {
                assert_eq!(1, 2);
            }
        }
        match dum2.send_to(&msg[..], &ip1).await {
            Ok(size) => {
                assert_eq!(size, msg.len());
            }
            Err(_) => {
                assert_eq!(2, 3);
            }
        }
        match dum3.send_to(&msg[..], &ip1).await {
            Ok(size) => {
                assert_eq!(size, msg.len());
            }
            Err(_) => {
                assert_eq!(3, 4);
            }
        }
        //test receiving
        let mut buf = [0u8;MTU];
        match dum1.recv_from(&mut buf).await {
            Ok((size, _)) => {
                assert_eq!(size, 0);
                assert_eq!(buf[0], 0);
            }
            Err(_) => {
                assert_eq!(1, 2);
            }
        }
        buf[0] = 0;
        match dum2.recv_from(&mut buf).await {
            Ok((size, _)) => {
                assert_eq!(size, 1);
                assert_eq!(buf[0], 1);
            }
            Err(_) => {
                assert_eq!(2, 3);
            }
        }
        buf[0] = 0;
        match dum3.recv_from(&mut buf).await {
            Ok((size, _)) => {
                assert_eq!(size, 0);
                assert_eq!(buf[0], 0);
            }
            Err(_) => {
                assert_eq!(3, 4);
            }
        }
    }
}
