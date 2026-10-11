// ABOUTME: Bounded peer-bound HTTP/UPnP control for the discovered Sonos coordinator.
// ABOUTME: Fresh observations validate identity but do not grant atomic playback ownership.
use crate::{
    sonos::DiscoveryGroup,
    stream_http::{DeadlineReader, DeadlineWriter},
};
use quick_xml::{events::Event, name::ResolveResult, NsReader};
use std::{
    io::{self, Read},
    net::{SocketAddr, TcpStream},
    time::Instant,
};
const CAP: usize = 32768;
const AV: &str = "urn:schemas-upnp-org:service:AVTransport:1";
const TOPO: &str = "urn:schemas-upnp-org:service:ZoneGroupTopology:1";
const SOAP: &str = "http://schemas.xmlsoap.org/soap/envelope/";
const DEVICE: &str = "urn:schemas-upnp-org:device-1-0";
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid Sonos HTTP/XML response",
    )
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Playing,
    Transitioning,
    Stopped,
    Paused,
}
pub struct Client {
    group: DiscoveryGroup,
    peer: SocketAddr,
    av: String,
    topology: String,
}
struct Node {
    name: String,
    ns: String,
    text: String,
    attrs: Vec<(String, String)>,
    parent: Option<usize>,
}
impl Node {
    fn attr(&self, name: &str) -> io::Result<&str> {
        let mut a = self.attrs.iter().filter(|(k, _)| k == name);
        let value = &a.next().ok_or_else(invalid)?.1;
        if a.next().is_some() {
            return Err(invalid());
        }
        Ok(value)
    }
}
fn xml(bytes: &[u8]) -> io::Result<Vec<Node>> {
    if bytes.len() > CAP {
        return Err(invalid());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let mut r = NsReader::from_str(text);
    r.config_mut().expand_empty_elements = true;
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack = Vec::new();
    let mut tokens = 0;
    let mut storage = 0;
    loop {
        tokens += 1;
        if tokens > 4096 {
            return Err(invalid());
        }
        let (ns, event) = r.read_resolved_event().map_err(|_| invalid())?;
        match event {
            Event::Start(e) => {
                if stack.len() >= 32
                    || nodes.len() >= 1024
                    || (stack.is_empty() && !nodes.is_empty())
                {
                    return Err(invalid());
                }
                let ns = match ns {
                    ResolveResult::Bound(n) => std::str::from_utf8(n.as_ref())
                        .map_err(|_| invalid())?
                        .to_owned(),
                    ResolveResult::Unbound => String::new(),
                    _ => return Err(invalid()),
                };
                let name = std::str::from_utf8(e.local_name().as_ref())
                    .map_err(|_| invalid())?
                    .to_owned();
                if name.len() > 256 || ns.len() > 256 {
                    return Err(invalid());
                }
                storage += name.len() + ns.len();
                let mut attrs = Vec::new();
                for a in e.attributes() {
                    let a = a.map_err(|_| invalid())?;
                    if attrs.len() >= 64 {
                        return Err(invalid());
                    }
                    attrs.push((
                        std::str::from_utf8(a.key.as_ref())
                            .map_err(|_| invalid())?
                            .into(),
                        a.decode_and_unescape_value(r.decoder())
                            .map_err(|_| invalid())?
                            .into_owned(),
                    ));
                }
                storage += attrs
                    .iter()
                    .map(|(k, v): &(String, String)| k.len() + v.len())
                    .sum::<usize>();
                if storage > CAP * 2 {
                    return Err(invalid());
                }
                nodes.push(Node {
                    name,
                    ns,
                    text: String::new(),
                    attrs,
                    parent: stack.last().copied(),
                });
                stack.push(nodes.len() - 1);
            }
            Event::End(_) => {
                stack.pop().ok_or_else(invalid)?;
            }
            Event::Text(t) => {
                let s = t.decode().map_err(|_| invalid())?;
                let s = quick_xml::escape::unescape(&s).map_err(|_| invalid())?;
                if let Some(i) = stack.last() {
                    nodes[*i].text.push_str(&s)
                } else if !s.trim().is_empty() {
                    return Err(invalid());
                }
            }
            Event::GeneralRef(e) => {
                let raw = e.decode().map_err(|_| invalid())?;
                let escaped = format!("&{raw};");
                let value = quick_xml::escape::unescape(&escaped).map_err(|_| invalid())?;
                nodes[*stack.last().ok_or_else(invalid)?]
                    .text
                    .push_str(&value);
            }
            Event::CData(t) => {
                nodes[*stack.last().ok_or_else(invalid)?]
                    .text
                    .push_str(&t.decode().map_err(|_| invalid())?);
            }
            Event::Eof => break,
            Event::Decl(_) | Event::Comment(_) => {}
            _ => return Err(invalid()),
        }
    }
    if !stack.is_empty() || nodes.is_empty() {
        return Err(invalid());
    }
    Ok(nodes)
}
fn child<'a>(nodes: &'a [Node], parent: usize, name: &str) -> io::Result<(usize, &'a Node)> {
    let mut found = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.parent == Some(parent) && n.name == name);
    let n = found.next().ok_or_else(invalid)?;
    if found.next().is_some() {
        return Err(invalid());
    }
    Ok(n)
}
fn value(nodes: &[Node], parent: usize, name: &str) -> io::Result<String> {
    let (i, n) = child(nodes, parent, name)?;
    if (!n.ns.is_empty() && n.ns != nodes[parent].ns) || nodes.iter().any(|c| c.parent == Some(i)) {
        return Err(invalid());
    }
    Ok(n.text.clone())
}
fn endpoint(peer: SocketAddr, url: &str) -> io::Result<String> {
    let path = if let Some(v) = url.strip_prefix("http://") {
        let (authority, path) = v.split_once('/').ok_or_else(invalid)?;
        if authority != peer.to_string() {
            return Err(invalid());
        }
        format!("/{path}")
    } else {
        url.into()
    };
    if !path.starts_with('/')
        || path.starts_with("//")
        || path
            .bytes()
            .any(|b| b <= 32 || b >= 127 || b == b'\\' || b == b'#')
    {
        return Err(invalid());
    }
    Ok(path)
}
fn remaining(deadline: Instant) -> io::Result<std::time::Duration> {
    let d = deadline.saturating_duration_since(Instant::now());
    if d.is_zero() {
        Err(io::Error::new(io::ErrorKind::TimedOut, "Sonos deadline"))
    } else {
        Ok(d)
    }
}
fn request(
    peer: SocketAddr,
    path: &str,
    action: Option<(&str, &str)>,
    body: &str,
    deadline: Instant,
) -> io::Result<Vec<u8>> {
    let socket = TcpStream::connect_timeout(&peer, remaining(deadline)?)?;
    let extra=action.map(|(service,method)|format!("Content-Type: text/xml; charset=\"utf-8\"\r\nSOAPAction: \"{service}#{method}\"\r\n")).unwrap_or_default();
    let req=format!("{} {path} HTTP/1.1\r\nHost: {peer}\r\nConnection: close\r\n{extra}Content-Length: {}\r\n\r\n{body}",if action.is_some(){"POST"}else{"GET"},body.len());
    DeadlineWriter::new(&socket).write_all_before(req.as_bytes(), deadline)?;
    response(DeadlineReader::new(&socket, deadline))
}
fn response(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut head = Vec::new();
    let mut byte = [0];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= 8192 {
            return Err(invalid());
        }
        reader.read_exact(&mut byte)?;
        head.push(byte[0]);
    }
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut parsed = httparse::Response::new(&mut headers);
    if !parsed.parse(&head).map_err(|_| invalid())?.is_complete() || parsed.code != Some(200) {
        return Err(invalid());
    }
    let mut length = None;
    let mut chunked = false;
    for h in parsed.headers.iter() {
        if h.name.eq_ignore_ascii_case("Content-Length") {
            if length.is_some() {
                return Err(invalid());
            }
            length = Some(
                std::str::from_utf8(h.value)
                    .map_err(|_| invalid())?
                    .parse::<usize>()
                    .map_err(|_| invalid())?,
            );
        }
        if h.name.eq_ignore_ascii_case("Transfer-Encoding") {
            if chunked || !h.value.eq_ignore_ascii_case(b"chunked") {
                return Err(invalid());
            }
            chunked = true;
        }
    }
    if chunked && length.is_some() {
        return Err(invalid());
    }
    let mut body = Vec::new();
    if chunked {
        loop {
            let mut line = Vec::new();
            while !line.ends_with(b"\r\n") {
                if line.len() >= 128 {
                    return Err(invalid());
                }
                reader.read_exact(&mut byte)?;
                line.push(byte[0]);
            }
            let text = std::str::from_utf8(&line[..line.len() - 2]).map_err(|_| invalid())?;
            let size = usize::from_str_radix(text, 16).map_err(|_| invalid())?;
            if size > CAP - body.len() {
                return Err(invalid());
            }
            if size == 0 {
                let mut end = [0; 2];
                reader.read_exact(&mut end)?;
                if end != *b"\r\n" {
                    return Err(invalid());
                }
                break;
            }
            let old = body.len();
            body.resize(old + size, 0);
            reader.read_exact(&mut body[old..])?;
            let mut end = [0; 2];
            reader.read_exact(&mut end)?;
            if end != *b"\r\n" {
                return Err(invalid());
            }
        }
    } else if let Some(n) = length {
        if n > CAP {
            return Err(invalid());
        }
        body.resize(n, 0);
        reader.read_exact(&mut body)?;
    } else {
        reader.take((CAP + 1) as u64).read_to_end(&mut body)?;
        if body.len() > CAP {
            return Err(invalid());
        }
    }
    Ok(body)
}
fn checked_io<T>(
    validate: &mut impl FnMut() -> io::Result<()>,
    operation: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    validate()?;
    let result = operation();
    validate()?;
    result
}
impl Client {
    pub fn connect(group: &DiscoveryGroup, deadline: Instant) -> io::Result<Self> {
        remaining(deadline)?;
        if !group.is_leader || group.player_id.is_empty() {
            return Err(invalid());
        }
        let peer = SocketAddr::from((group.host, 1400));
        let mut c = Self {
            group: group.clone(),
            peer,
            av: String::new(),
            topology: String::new(),
        };
        let (av, topology) = c.description(deadline)?;
        c.av = av;
        c.topology = topology;
        c.verify(deadline)?;
        Ok(c)
    }
    fn description(&self, deadline: Instant) -> io::Result<(String, String)> {
        let nodes = xml(&request(
            self.peer,
            "/xml/device_description.xml",
            None,
            "",
            deadline,
        )?)?;
        if nodes[0].name != "root" || nodes[0].ns != DEVICE {
            return Err(invalid());
        }
        let (device, _) = child(&nodes, 0, "device")?;
        if value(&nodes, device, "UDN")? != format!("uuid:{}", self.group.player_id) {
            return Err(invalid());
        }
        let mut av = None;
        let mut topology = None;
        for (i, n) in nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.name == "service" && n.ns == DEVICE)
        {
            let typ = value(&nodes, i, "serviceType")?;
            if typ == AV || typ == TOPO {
                let list = n.parent.ok_or_else(invalid)?;
                let owner = nodes[list].parent.ok_or_else(invalid)?;
                if nodes[list].name != "serviceList"
                    || nodes[list].ns != DEVICE
                    || nodes[owner].name != "device"
                    || nodes[owner].ns != DEVICE
                {
                    return Err(invalid());
                }
                if typ == TOPO {
                    if owner != device {
                        return Err(invalid());
                    }
                } else {
                    let device_list = nodes[owner].parent.ok_or_else(invalid)?;
                    if nodes[device_list].name != "deviceList"
                        || nodes[device_list].parent != Some(device)
                        || value(&nodes, owner, "UDN")?
                            != format!("uuid:{}_MR", self.group.player_id)
                    {
                        return Err(invalid());
                    }
                }
                let path = endpoint(self.peer, &value(&nodes, i, "controlURL")?)?;
                let slot = if typ == AV { &mut av } else { &mut topology };
                if slot.replace(path).is_some() {
                    return Err(invalid());
                }
            }
        }
        Ok((av.ok_or_else(invalid)?, topology.ok_or_else(invalid)?))
    }
    fn soap(
        &self,
        service: &str,
        path: &str,
        action: &str,
        fields: &str,
        deadline: Instant,
    ) -> io::Result<Vec<Node>> {
        let body=format!("<s:Envelope xmlns:s=\"{SOAP}\"><s:Body><u:{action} xmlns:u=\"{service}\">{fields}</u:{action}></s:Body></s:Envelope>");
        let nodes = xml(&request(
            self.peer,
            path,
            Some((service, action)),
            &body,
            deadline,
        )?)?;
        if nodes[0].name != "Envelope" || nodes[0].ns != SOAP {
            return Err(invalid());
        }
        let (b, bn) = child(&nodes, 0, "Body")?;
        if bn.ns != SOAP {
            return Err(invalid());
        }
        let (result, n) = child(&nodes, b, &format!("{action}Response"))?;
        if n.ns != service || nodes.iter().filter(|n| n.parent == Some(b)).count() != 1 {
            return Err(invalid());
        }
        // Return the action subtree with its root at zero, retaining namespace checks above.
        let offset = result;
        Ok(nodes
            .into_iter()
            .skip(offset)
            .map(|mut n| {
                n.parent = n.parent.and_then(|p| p.checked_sub(offset));
                n
            })
            .collect())
    }
    pub fn verify(&self, deadline: Instant) -> io::Result<()> {
        self.verify_checked(deadline, &mut || Ok(()))
    }
    fn verify_checked(
        &self,
        deadline: Instant,
        validate: &mut impl FnMut() -> io::Result<()>,
    ) -> io::Result<()> {
        let (av, topology) = checked_io(validate, || self.description(deadline))?;
        if av != self.av || topology != self.topology {
            return Err(invalid());
        }
        let outer = checked_io(validate, || {
            self.soap(TOPO, &self.topology, "GetZoneGroupState", "", deadline)
        })?;
        let nodes = xml(value(&outer, 0, "ZoneGroupState")?.as_bytes())?;
        if nodes[0].name != "ZoneGroupState" || !nodes[0].ns.is_empty() {
            return Err(invalid());
        }
        let (groups, groups_node) = child(&nodes, 0, "ZoneGroups")?;
        if !groups_node.ns.is_empty() {
            return Err(invalid());
        }
        // Identity-bearing elements only carry authority in the retained UPnP hierarchy.
        for node in &nodes {
            let valid = match node.name.as_str() {
                "ZoneGroupState" => node.parent.is_none(),
                "ZoneGroups" => node.parent == Some(0),
                "ZoneGroup" => node.parent == Some(groups),
                "ZoneGroupMember" => node.parent.is_some_and(|parent| {
                    nodes[parent].name == "ZoneGroup" && nodes[parent].parent == Some(groups)
                }),
                _ => continue,
            };
            if !valid || !node.ns.is_empty() {
                return Err(invalid());
            }
        }
        let mut matches = 0;
        let mut ids = std::collections::HashSet::new();
        for (i, n) in nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.name == "ZoneGroup")
        {
            let coordinator = n.attr("Coordinator")?;
            let id = n.attr("ID")?;
            for member in nodes
                .iter()
                .filter(|m| m.parent == Some(i) && m.name == "ZoneGroupMember")
            {
                let uuid = member.attr("UUID")?;
                if !ids.insert(uuid) {
                    return Err(invalid());
                }
                if uuid == self.group.player_id {
                    matches += 1;
                    if coordinator != uuid
                        || id != self.group.group_id
                        || endpoint(self.peer, member.attr("Location")?)?
                            != "/xml/device_description.xml"
                    {
                        return Err(invalid());
                    }
                }
            }
        }
        if matches != 1 {
            return Err(invalid());
        }
        Ok(())
    }
    /// Observe retained identity and URI before reading transport, without media writes.
    /// Validation surrounds every request so late replies cannot restore canceled work.
    pub fn observe(
        &self,
        expected_uri: &str,
        deadline: Instant,
        mut validate: impl FnMut() -> io::Result<()>,
    ) -> io::Result<Option<TransportState>> {
        self.verify_checked(deadline, &mut validate)?;
        let uri = checked_io(&mut validate, || self.media_uri(deadline))?;
        if uri != expected_uri {
            return Ok(None);
        }
        checked_io(&mut validate, || self.transport_state(deadline)).map(Some)
    }
    pub fn media_uri(&self, deadline: Instant) -> io::Result<String> {
        value(
            &self.soap(
                AV,
                &self.av,
                "GetMediaInfo",
                "<InstanceID>0</InstanceID>",
                deadline,
            )?,
            0,
            "CurrentURI",
        )
    }
    pub fn transport_state(&self, deadline: Instant) -> io::Result<TransportState> {
        match value(
            &self.soap(
                AV,
                &self.av,
                "GetTransportInfo",
                "<InstanceID>0</InstanceID>",
                deadline,
            )?,
            0,
            "CurrentTransportState",
        )?
        .as_str()
        {
            "PLAYING" => Ok(TransportState::Playing),
            "TRANSITIONING" => Ok(TransportState::Transitioning),
            "STOPPED" => Ok(TransportState::Stopped),
            "PAUSED_PLAYBACK" => Ok(TransportState::Paused),
            _ => Err(invalid()),
        }
    }
    pub fn set_uri(&self, uri: &str, deadline: Instant) -> io::Result<()> {
        if uri.len() > 4096 || uri.chars().any(char::is_control) {
            return Err(invalid());
        }
        let fields=format!("<InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI><CurrentURIMetaData></CurrentURIMetaData>",quick_xml::escape::escape(uri));
        self.soap(AV, &self.av, "SetAVTransportURI", &fields, deadline)?;
        Ok(())
    }
    pub fn play(&self, deadline: Instant) -> io::Result<()> {
        self.soap(
            AV,
            &self.av,
            "Play",
            "<InstanceID>0</InstanceID><Speed>1</Speed>",
            deadline,
        )?;
        Ok(())
    }
    pub fn stop(&self, deadline: Instant) -> io::Result<()> {
        self.soap(AV, &self.av, "Stop", "<InstanceID>0</InstanceID>", deadline)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        net::{Ipv4Addr, TcpListener},
        thread,
        time::Duration,
    };
    const DESCRIPTION: &str = include_str!("../tests/fixtures/sonos/device.xml");
    const MEDIA: &str = include_str!("../tests/fixtures/sonos/media.xml");
    fn client(peer: SocketAddr) -> Client {
        Client {
            group: DiscoveryGroup {
                player_id: "RINCON_TEST".into(),
                group_id: "RINCON_GROUP:1".into(),
                household_id: "house".into(),
                name: "test".into(),
                host: Ipv4Addr::LOCALHOST,
                is_leader: true,
            },
            peer,
            av: "/MediaRenderer/AVTransport/Control".into(),
            topology: "/ZoneGroupTopology/Control".into(),
        }
    }
    fn serve(bodies: Vec<String>) -> (SocketAddr, thread::JoinHandle<Vec<String>>) {
        serve_replies(bodies, Duration::ZERO, false)
    }
    fn serve_replies(
        bodies: Vec<String>,
        delay: Duration,
        truncate_last: bool,
    ) -> (SocketAddr, thread::JoinHandle<Vec<String>>) {
        let last = bodies.len() - 1;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let peer = listener.local_addr().unwrap();
        let h = thread::spawn(move || {
            bodies
                .into_iter()
                .enumerate()
                .filter_map(|(index, body)| {
                    let body = body.replace("$PEER", &peer.to_string());
                    let accept_deadline = Instant::now() + Duration::from_secs(2);
                    let (mut sock, _) = loop {
                        match listener.accept() {
                            Ok(connection) => break connection,
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                                if Instant::now() >= accept_deadline {
                                    return None;
                                }
                                thread::sleep(Duration::from_millis(1));
                            }
                            Err(error) => panic!("loopback accept failed: {error}"),
                        }
                    };
                    sock.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                    let mut bytes = Vec::new();
                    let mut b = [0];
                    while !bytes.ends_with(b"\r\n\r\n") {
                        sock.read_exact(&mut b).unwrap();
                        bytes.push(b[0]);
                    }
                    let h = String::from_utf8(bytes.clone()).unwrap();
                    let n = h
                        .lines()
                        .find_map(|l| l.strip_prefix("Content-Length: "))
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    let mut payload = vec![0; n];
                    sock.read_exact(&mut payload).unwrap();
                    bytes.extend(payload);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    );
                    thread::sleep(delay);
                    let response = if truncate_last && index == last {
                        &response.as_bytes()[..response.len() - 8]
                    } else {
                        response.as_bytes()
                    };
                    for chunk in response.chunks(17) {
                        if sock.write_all(chunk).is_err() {
                            break;
                        }
                    }
                    Some(String::from_utf8(bytes).unwrap())
                })
                .collect()
        });
        (peer, h)
    }
    fn observation_bodies(media: String) -> Vec<String> {
        vec![
            DESCRIPTION.into(),
            include_str!("../tests/fixtures/sonos/topology.xml").replace("127.0.0.1:1400", "$PEER"),
            media,
        ]
    }
    const EXPECTED_URI: &str = "http://127.0.0.1/stream.mp3?network=1&intent=2";
    #[test]
    fn observation_is_ordered_read_only_and_checks_each_io() {
        let mut bodies = observation_bodies(MEDIA.into());
        bodies.push(include_str!("../tests/fixtures/sonos/transport.xml").into());
        let (peer, handle) = serve(bodies);
        let mut checks = 0;
        let state = client(peer)
            .observe(
                EXPECTED_URI,
                Instant::now() + Duration::from_secs(2),
                || {
                    checks += 1;
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(state, Some(TransportState::Transitioning));
        assert_eq!(checks, 8);
        let requests = handle.join().unwrap();
        assert!(requests[0].starts_with("GET /xml/device_description.xml HTTP/1.1"));
        for (request, action) in
            requests[1..]
                .iter()
                .zip(["GetZoneGroupState", "GetMediaInfo", "GetTransportInfo"])
        {
            assert!(request.starts_with("POST "));
            assert!(request.contains(&format!("#{action}\"")));
            for mutation in ["#Play\"", "#Stop\"", "#SetAVTransportURI\""] {
                assert!(!request.contains(mutation));
            }
        }
    }
    #[test]
    fn observation_foreign_uri_stops_before_transport() {
        let (peer, handle) = serve(observation_bodies(MEDIA.into()));
        assert_eq!(
            client(peer)
                .observe(
                    "foreign",
                    Instant::now() + Duration::from_secs(2),
                    || Ok(())
                )
                .unwrap(),
            None
        );
        assert_eq!(handle.join().unwrap().len(), 3);
    }
    #[test]
    fn observation_cancellation_after_description_prevents_topology() {
        let (peer, handle) = serve(vec![DESCRIPTION.into()]);
        let mut checks = 0;
        let error = client(peer)
            .observe(
                EXPECTED_URI,
                Instant::now() + Duration::from_secs(2),
                || {
                    checks += 1;
                    if checks == 2 {
                        Err(io::Error::from(io::ErrorKind::Interrupted))
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(handle.join().unwrap().len(), 1);
    }
    #[test]
    fn observation_rejects_malformed_media_before_transport() {
        let (peer, handle) = serve(observation_bodies("<broken>".into()));
        assert_eq!(
            client(peer)
                .observe(
                    EXPECTED_URI,
                    Instant::now() + Duration::from_secs(2),
                    || Ok(())
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(handle.join().unwrap().len(), 3);
    }
    #[test]
    fn observation_rejects_changed_identity_before_media() {
        let mut bodies = observation_bodies(MEDIA.into());
        bodies.truncate(2);
        bodies[1] = bodies[1].replace("Coordinator=\"RINCON_TEST\"", "Coordinator=\"FOREIGN\"");
        let (peer, handle) = serve(bodies);
        assert_eq!(
            client(peer)
                .observe(
                    EXPECTED_URI,
                    Instant::now() + Duration::from_secs(2),
                    || Ok(())
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(handle.join().unwrap().len(), 2);
    }
    #[test]
    fn observation_rejects_unknown_transport_state() {
        let mut bodies = observation_bodies(MEDIA.into());
        bodies.push(
            include_str!("../tests/fixtures/sonos/transport.xml")
                .replace("TRANSITIONING", "UNKNOWN"),
        );
        let (peer, handle) = serve(bodies);
        assert_eq!(
            client(peer)
                .observe(
                    EXPECTED_URI,
                    Instant::now() + Duration::from_secs(2),
                    || Ok(())
                )
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(handle.join().unwrap().len(), 4);
    }
    #[test]
    fn observation_rejects_truncated_http_body() {
        let (peer, handle) = serve_replies(observation_bodies(MEDIA.into()), Duration::ZERO, true);
        let mut checks = 0;
        let error = client(peer)
            .observe(
                EXPECTED_URI,
                Instant::now() + Duration::from_secs(2),
                || {
                    checks += 1;
                    Ok(())
                },
            )
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(checks, 6);
        assert_eq!(handle.join().unwrap().len(), 3);
    }
    #[test]
    fn observation_shares_one_deadline_across_requests() {
        let (peer, handle) = serve_replies(
            observation_bodies(MEDIA.into()),
            Duration::from_millis(100),
            false,
        );
        let start = Instant::now();
        let error = client(peer)
            .observe(EXPECTED_URI, start + Duration::from_millis(250), || Ok(()))
            .unwrap_err();
        assert!(matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ));
        assert!(start.elapsed() < Duration::from_millis(700));
        handle.join().unwrap();
    }
    #[test]
    fn fixture_media_and_escaped_uri_use_real_socket() {
        let (peer, h) = serve(vec![
            MEDIA.into(),
            include_str!("../tests/fixtures/sonos/set.xml").into(),
        ]);
        let c = client(peer);
        let d = Instant::now() + Duration::from_secs(2);
        assert_eq!(
            c.media_uri(d).unwrap(),
            "http://127.0.0.1/stream.mp3?network=1&intent=2"
        );
        c.set_uri("http://host/stream.flac?a=1&b=<hi>", d).unwrap();
        let requests = h.join().unwrap();
        assert!(requests[1].contains("a=1&amp;b=&lt;hi&gt;"));
        assert!(requests[1].contains("<CurrentURIMetaData></CurrentURIMetaData>"));
    }
    #[test]
    fn actual_description_and_topology_validate() {
        let topology =
            include_str!("../tests/fixtures/sonos/topology.xml").replace("127.0.0.1:1400", "$PEER");
        let (peer, h) = serve(vec![DESCRIPTION.into(), topology]);
        let c = client(peer);
        c.verify(Instant::now() + Duration::from_secs(2)).unwrap();
        h.join().unwrap();
    }
    #[test]
    fn foreign_namespace_field_rejects() {
        let body = MEDIA.replace("<CurrentURI>", "<CurrentURI xmlns=\"urn:foreign\">");
        let (peer, h) = serve(vec![body]);
        assert!(client(peer)
            .media_uri(Instant::now() + Duration::from_secs(2))
            .is_err());
        h.join().unwrap();
    }
    #[test]
    fn foreign_embedded_device_service_rejects() {
        let body = DESCRIPTION.replace("uuid:RINCON_TEST_MR", "uuid:FOREIGN_MR");
        let (peer, h) = serve(vec![body]);
        assert!(client(peer)
            .description(Instant::now() + Duration::from_secs(2))
            .is_err());
        h.join().unwrap();
    }
    #[test]
    fn transport_and_mutation_actions_match_receipts() {
        let (peer, h) = serve(vec![
            include_str!("../tests/fixtures/sonos/transport.xml").into(),
            include_str!("../tests/fixtures/sonos/play.xml").into(),
            include_str!("../tests/fixtures/sonos/stop.xml").into(),
        ]);
        let c = client(peer);
        let d = Instant::now() + Duration::from_secs(2);
        assert_eq!(c.transport_state(d).unwrap(), TransportState::Transitioning);
        c.play(d).unwrap();
        c.stop(d).unwrap();
        let requests = h.join().unwrap();
        assert!(requests[1].contains("<InstanceID>0</InstanceID><Speed>1</Speed>"));
        assert!(requests[2].contains("#Stop\""));
    }
    #[test]
    fn changed_identity_rejects() {
        let good =
            include_str!("../tests/fixtures/sonos/topology.xml").replace("127.0.0.1:1400", "$PEER");
        let cases = vec![
            good.replace("Coordinator=\"RINCON_TEST\"", "Coordinator=\"FOREIGN\""),
            good.replace("RINCON_GROUP:1", "changed"),
            good.replace(
                "http://$PEER/xml/device_description.xml",
                "http://foreign:1400/xml/device_description.xml",
            ),
        ];
        for body in cases {
            let (peer, h) = serve(vec![DESCRIPTION.into(), body]);
            assert!(client(peer)
                .verify(Instant::now() + Duration::from_secs(2))
                .is_err());
            h.join().unwrap();
        }
    }
    #[test]
    fn topology_foreign_namespaces_reject() {
        let good =
            include_str!("../tests/fixtures/sonos/topology.xml").replace("127.0.0.1:1400", "$PEER");
        for element in [
            "ZoneGroupState",
            "ZoneGroups",
            "ZoneGroup",
            "ZoneGroupMember",
        ] {
            let start = if element == "ZoneGroup" || element == "ZoneGroupMember" {
                format!("&lt;{element} ")
            } else {
                format!("&lt;{element}&gt;")
            };
            let replacement = if element == "ZoneGroup" || element == "ZoneGroupMember" {
                format!("&lt;{element} xmlns=\"urn:foreign\" ")
            } else {
                format!("&lt;{element} xmlns=\"urn:foreign\"&gt;")
            };
            assert!(good.contains(&start));
            let (peer, handle) =
                serve(vec![DESCRIPTION.into(), good.replace(&start, &replacement)]);
            assert!(
                client(peer)
                    .verify(Instant::now() + Duration::from_secs(2))
                    .is_err(),
                "{element}"
            );
            handle.join().unwrap();
        }
    }
    #[test]
    fn topology_unrelated_parent_rejects() {
        let good =
            include_str!("../tests/fixtures/sonos/topology.xml").replace("127.0.0.1:1400", "$PEER");
        let cases = [
            good.replace("&lt;ZoneGroups&gt;", "&lt;ZoneGroups&gt;&lt;Extension&gt;")
                .replace(
                    "&lt;/ZoneGroups&gt;",
                    "&lt;/Extension&gt;&lt;/ZoneGroups&gt;",
                ),
            good.replace(
                "&lt;ZoneGroupMember UUID=\"RINCON_TEST\"",
                "&lt;Extension&gt;&lt;ZoneGroupMember UUID=\"RINCON_TEST\"",
            )
            .replace(
                "$PEER/xml/device_description.xml\" /&gt;",
                "$PEER/xml/device_description.xml\" /&gt;&lt;/Extension&gt;",
            ),
            good.replace("&lt;ZoneGroupState&gt;", "&lt;foreign&gt;")
                .replace("&lt;/ZoneGroupState&gt;", "&lt;/foreign&gt;"),
        ];
        for body in cases {
            let (peer, handle) = serve(vec![DESCRIPTION.into(), body]);
            assert!(client(peer)
                .verify(Instant::now() + Duration::from_secs(2))
                .is_err());
            handle.join().unwrap();
        }
    }
    #[test]
    fn bounded_xml_rejects_invalid_and_dtd() {
        for s in [
            "<a><b></a>",
            "<!DOCTYPE a><a/>",
            "<a/><a/>",
            "<a>&unknown;</a>",
            "<a>",
        ] {
            assert!(xml(s.as_bytes()).is_err(), "{s}")
        }
        assert!(xml("x".repeat(CAP + 1).as_bytes()).is_err());
        assert!(xml(format!("{}{}", "<a>".repeat(33), "</a>".repeat(33)).as_bytes()).is_err());
    }
    #[test]
    fn endpoints_remain_peer_bound() {
        let peer = "127.0.0.1:1400".parse().unwrap();
        for s in [
            "http://evil:1400/a",
            "http://user@127.0.0.1:1400/a",
            "//evil/a",
            "/a\r\nHost: evil",
            "https://127.0.0.1:1400/a",
            "/a#frag",
        ] {
            assert!(endpoint(peer, s).is_err(), "{s}")
        }
        assert_eq!(endpoint(peer, "http://127.0.0.1:1400/a").unwrap(), "/a");
    }
    #[test]
    fn framing_rejects_partial_ambiguous_redirect_and_oversized() {
        for s in [
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\na",
            "HTTP/1.1 302 Found\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 32769\r\n\r\n",
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 0\r\n\r\n",
        ] {
            assert!(response(s.as_bytes()).is_err(), "{s}")
        }
    }
    #[test]
    fn chunked_and_eof_complete() {
        assert_eq!(
            response(
                &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nhi\r\n0\r\n\r\n"[..]
            )
            .unwrap(),
            b"hi"
        );
        assert_eq!(response(&b"HTTP/1.0 200 OK\r\n\r\nhi"[..]).unwrap(), b"hi");
    }
    #[test]
    fn soap_fault_wrong_action_and_duplicate_uri_reject() {
        let wrong = MEDIA.replace("GetMediaInfoResponse", "GetTransportInfoResponse");
        let duplicate = MEDIA.replace(
            "</CurrentURI>",
            "</CurrentURI><CurrentURI>foreign</CurrentURI>",
        );
        let fault=format!("<s:Envelope xmlns:s=\"{SOAP}\"><s:Body><s:Fault><faultcode>s:Client</faultcode></s:Fault></s:Body></s:Envelope>");
        let (peer, h) = serve(vec![wrong, duplicate, fault]);
        let c = client(peer);
        for _ in 0..3 {
            assert!(c
                .media_uri(Instant::now() + Duration::from_secs(2))
                .is_err())
        }
        h.join().unwrap();
    }
    #[test]
    fn trickled_socket_obeys_absolute_deadline() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let peer = listener.local_addr().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            for b in b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n" {
                if s.write_all(&[*b]).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        });
        let start = Instant::now();
        assert!(request(peer, "/", None, "", start + Duration::from_millis(70)).is_err());
        assert!(start.elapsed() < Duration::from_millis(500));
        h.join().unwrap();
    }
}
