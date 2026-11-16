// Who is connected and which servers they listen to. Sends never wait: every session
// has a bounded outbox drained by its own websocket task, and a session whose outbox
// is full is dropped (its socket closes, the client reconnects and refetches).

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};
use tokio::sync::mpsc;
use uuid::Uuid;
use ws::Message;

const OUTBOX: usize = 256;

struct Session {
    user_id: i32,
    servers: HashSet<i32>,
    tx: mpsc::Sender<Message>,
}

#[derive(Default)]
struct HubState {
    sessions: HashMap<Uuid, Session>,
    by_user: HashMap<i32, HashSet<Uuid>>,
    by_server: HashMap<i32, HashSet<Uuid>>,
    // bumped by every leave and server delete; see subscribe
    leaves: u64,
}

// ponytail: one std Mutex, never held across an await; shard by server if it ever contends
#[derive(Default)]
pub struct Hub(Mutex<HubState>);

impl Hub {
    fn lock(&self) -> MutexGuard<'_, HubState> {
        self.0.lock().unwrap()
    }

    // a new session gets joins right away; subscribe it to the user's servers next
    pub fn connect(&self, user_id: i32) -> (Uuid, mpsc::Receiver<Message>) {
        let id = Uuid::new_v4();
        let (tx, rx) = mpsc::channel(OUTBOX);
        let mut s = self.lock();
        s.by_user.entry(user_id).or_default().insert(id);
        s.sessions.insert(
            id,
            Session {
                user_id,
                servers: HashSet::new(),
                tx,
            },
        );
        (id, rx)
    }

    pub fn leaves(&self) -> u64 {
        self.lock().leaves
    }

    // Subscribes a session to servers read from the db after `leaves()` returned `leaves`.
    // False if a leave or server delete happened since: the list may hold a server the
    // user already left, so read it again.
    // ponytail: one global counter, so any leave anywhere forces a retry; per-user if that ever spins
    pub fn subscribe(&self, id: Uuid, server_ids: &[i32], leaves: u64) -> bool {
        let mut s = self.lock();
        if s.leaves != leaves {
            return false;
        }
        let HubState {
            sessions,
            by_server,
            ..
        } = &mut *s;
        // a dropped session's task ends on its closed outbox
        if let Some(session) = sessions.get_mut(&id) {
            for server_id in server_ids {
                session.servers.insert(*server_id);
                by_server.entry(*server_id).or_default().insert(id);
            }
        }
        true
    }

    pub fn disconnect(&self, id: Uuid) {
        self.lock().remove(id);
    }

    pub fn join_server(&self, user_id: i32, server_id: i32) {
        let mut s = self.lock();
        let HubState {
            sessions,
            by_user,
            by_server,
            ..
        } = &mut *s;
        for id in by_user.get(&user_id).into_iter().flatten() {
            sessions.get_mut(id).unwrap().servers.insert(server_id);
            by_server.entry(server_id).or_default().insert(*id);
        }
    }

    pub fn leave_server(&self, user_id: i32, server_id: i32) {
        let mut s = self.lock();
        s.leaves += 1;
        let HubState {
            sessions,
            by_user,
            by_server,
            ..
        } = &mut *s;
        for id in by_user.get(&user_id).into_iter().flatten() {
            sessions.get_mut(id).unwrap().servers.remove(&server_id);
            unindex(by_server, server_id, *id);
        }
    }

    pub fn remove_server(&self, server_id: i32) {
        let mut s = self.lock();
        s.leaves += 1;
        for id in s.by_server.remove(&server_id).into_iter().flatten() {
            s.sessions.get_mut(&id).unwrap().servers.remove(&server_id);
        }
    }

    pub fn send_to_server(&self, server_id: i32, text: &str) {
        let mut s = self.lock();
        let ids: Vec<Uuid> = s
            .by_server
            .get(&server_id)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        s.send(&ids, Message::text(text));
    }

    pub fn send_to_users(&self, user_ids: &[i32], text: &str) {
        let mut s = self.lock();
        let ids: Vec<Uuid> = user_ids
            .iter()
            .filter_map(|user_id| s.by_user.get(user_id))
            .flatten()
            .copied()
            .collect();
        s.send(&ids, Message::text(text));
    }

    // false once the session is gone
    pub fn send_to_session(&self, id: Uuid, text: String) -> bool {
        let mut s = self.lock();
        s.send(&[id], Message::Text(text));
        s.sessions.contains_key(&id)
    }
}

impl HubState {
    fn send(&mut self, ids: &[Uuid], msg: Message) {
        for id in ids {
            let sent = self
                .sessions
                .get(id)
                .is_some_and(|s| s.tx.try_send(msg.clone()).is_ok());
            if !sent {
                self.remove(*id);
            }
        }
    }

    // dropping the Sender ends the session's websocket task
    fn remove(&mut self, id: Uuid) {
        let Some(session) = self.sessions.remove(&id) else {
            return;
        };
        unindex(&mut self.by_user, session.user_id, id);
        for server_id in session.servers {
            unindex(&mut self.by_server, server_id, id);
        }
    }
}

fn unindex(index: &mut HashMap<i32, HashSet<Uuid>>, key: i32, id: Uuid) {
    if let Some(ids) = index.get_mut(&key) {
        ids.remove(&id);
        if ids.is_empty() {
            index.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connect(hub: &Hub, user_id: i32, servers: &[i32]) -> (Uuid, mpsc::Receiver<Message>) {
        let (id, rx) = hub.connect(user_id);
        assert!(hub.subscribe(id, servers, hub.leaves()));
        (id, rx)
    }

    fn drain(rx: &mut mpsc::Receiver<Message>) -> Vec<String> {
        std::iter::from_fn(|| rx.try_recv().ok())
            .map(|m| m.into_text().unwrap())
            .collect()
    }

    #[test]
    fn fans_out_to_every_tab_of_a_user() {
        let hub = Hub::default();
        let (_, mut tab1) = connect(&hub, 1, &[10]);
        let (_, mut tab2) = connect(&hub, 1, &[10]);
        let (_, mut other) = connect(&hub, 2, &[]);

        hub.send_to_server(10, "a");
        hub.send_to_users(&[1], "b");
        assert_eq!(drain(&mut tab1), ["a", "b"]);
        assert_eq!(drain(&mut tab2), ["a", "b"]);
        assert!(drain(&mut other).is_empty());

        // joining subscribes every open tab
        hub.join_server(2, 10);
        hub.send_to_server(10, "c");
        assert_eq!(drain(&mut other), ["c"]);
    }

    #[test]
    fn leave_and_remove_server_stop_delivery() {
        let hub = Hub::default();
        let (_, mut a) = connect(&hub, 1, &[10, 20]);
        let (_, mut b) = connect(&hub, 2, &[10, 20]);

        hub.leave_server(1, 10);
        hub.send_to_server(10, "x");
        assert!(drain(&mut a).is_empty());
        assert_eq!(drain(&mut b), ["x"]);

        hub.remove_server(20);
        hub.send_to_server(20, "y");
        assert!(drain(&mut a).is_empty());
        assert!(drain(&mut b).is_empty());
        assert!(!hub.lock().by_server.contains_key(&20));
    }

    #[test]
    fn stale_server_list_is_refused() {
        let hub = Hub::default();
        let (id, mut rx) = hub.connect(1);
        let leaves = hub.leaves();
        // the user leaves server 10 after its server list was read
        hub.leave_server(1, 10);
        assert!(!hub.subscribe(id, &[10], leaves));
        // the reread list no longer has it
        assert!(hub.subscribe(id, &[], hub.leaves()));
        hub.send_to_server(10, "x");
        assert!(drain(&mut rx).is_empty());
    }

    #[test]
    fn full_outbox_drops_only_that_session() {
        let hub = Hub::default();
        let (slow, _slow_rx) = connect(&hub, 1, &[10]);
        let (fast, mut fast_rx) = connect(&hub, 2, &[10]);

        for _ in 0..OUTBOX {
            hub.send_to_server(10, "m");
            drain(&mut fast_rx);
        }
        assert!(hub.send_to_session(fast, "ok".into()));
        hub.send_to_server(10, "overflow");

        assert!(!hub.send_to_session(slow, "gone".into()));
        assert_eq!(drain(&mut fast_rx), ["ok", "overflow"]);
        let s = hub.lock();
        assert!(!s.sessions.contains_key(&slow));
        assert_eq!(s.by_server[&10], HashSet::from([fast]));
        assert!(!s.by_user.contains_key(&1));
    }

    #[test]
    fn disconnect_cleans_every_index() {
        let hub = Hub::default();
        let (id, mut rx) = connect(&hub, 1, &[10]);
        hub.join_server(1, 20);
        hub.disconnect(id);

        let s = hub.lock();
        assert!(s.sessions.is_empty() && s.by_user.is_empty() && s.by_server.is_empty());
        drop(s);
        // the outbox closes, which is what ends the websocket task
        assert!(rx.try_recv().is_err() && rx.is_closed());
        assert!(!hub.send_to_session(id, "x".into()));
    }
}
