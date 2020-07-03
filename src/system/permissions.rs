use std::sync::
{
    Arc,
    Mutex
};

#[derive(Debug, Clone)]
pub struct Permissions(Arc<Mutex<u32>>);

impl Permissions
{
    fn new(mode: u32) -> Self
    {
        Permissions(Arc::new(Mutex::new(mode)))
    }

    pub fn get(&self) -> u32
    {
        *self.0.lock().unwrap()
    }

    pub fn set(&self, mode: u32)
    {
        *self.0.lock().unwrap() = mode;
    }

    pub fn can_read(&self) -> bool
    {
        (*self.0.lock().unwrap() & 0o444) != 0
    }

    pub fn can_write(&self) -> bool
    {
        (*self.0.lock().unwrap() & 0o222) != 0
    }

    pub fn can_execute(&self) -> bool
    {
        (*self.0.lock().unwrap() & 0o111) != 0
    }

    pub fn make_read_only(&self)
    {
        let mut mode = self.0.lock().unwrap();
        *mode &= !0o222;
    }
}

#[derive(Debug)]
pub struct Dir {
    pub mode: Permissions,
}

impl Default for Dir
{
    fn default() -> Self
    {
        Dir
        {
            mode: Permissions::new(0o644)
        }
    }
}

#[cfg(test)]
mod tests
{
    use crate::system::permissions::
    {
        Permissions
    };

    #[test]
    fn mode_gets_whats_put()
    {
        let mode = Permissions::new(0o123);
        assert_eq!(mode.get(), 0o123);
    }

    #[test]
    fn mode_gets_whats_set()
    {
        let mode = Permissions::new(0o123);
        mode.set(0o371);
        assert_eq!(mode.get(), 0o371);
    }

    #[test]
    fn can_read_agrees_with_numbers()
    {
        assert!(!Permissions::new(0o001).can_read());
        assert!(!Permissions::new(0o002).can_read());
        assert!( Permissions::new(0o004).can_read());
        assert!(!Permissions::new(0o010).can_read());
        assert!(!Permissions::new(0o020).can_read());
        assert!( Permissions::new(0o040).can_read());
        assert!(!Permissions::new(0o100).can_read());
        assert!(!Permissions::new(0o200).can_read());
        assert!( Permissions::new(0o400).can_read());
    }

    #[test]
    fn can_write_agrees_with_numbers()
    {
        assert!(!Permissions::new(0o001).can_write());
        assert!( Permissions::new(0o002).can_write());
        assert!(!Permissions::new(0o004).can_write());
        assert!(!Permissions::new(0o010).can_write());
        assert!( Permissions::new(0o020).can_write());
        assert!(!Permissions::new(0o040).can_write());
        assert!(!Permissions::new(0o100).can_write());
        assert!( Permissions::new(0o200).can_write());
        assert!(!Permissions::new(0o400).can_write());
    }

    #[test]
    fn can_execute_agrees_with_numbers()
    {
        assert!( Permissions::new(0o001).can_execute());
        assert!(!Permissions::new(0o002).can_execute());
        assert!(!Permissions::new(0o004).can_execute());
        assert!( Permissions::new(0o010).can_execute());
        assert!(!Permissions::new(0o020).can_execute());
        assert!(!Permissions::new(0o040).can_execute());
        assert!( Permissions::new(0o100).can_execute());
        assert!(!Permissions::new(0o200).can_execute());
        assert!(!Permissions::new(0o400).can_execute());
    }

    #[test]
    fn make_read_only_can_write_false()
    {
        let mode = Permissions::new(0o777);
        mode.make_read_only();

        assert!(mode.can_read());
        assert!(!mode.can_write());
        assert!(mode.can_execute());
    }
}

