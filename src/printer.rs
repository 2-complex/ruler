
use std::io::Write;
use termcolor::
{
    Color,
    ColorChoice,
    ColorSpec,
    StandardStream,
    WriteColor
};

pub trait Printer
{
    fn print_single_banner_line(
        &mut self, banner_text : &str, banner_color : Color, path : &str);

    fn print(
        &mut self, text : &str);

    fn error(
        &mut self, text: &str);
}

pub struct StandardPrinter
{
}

impl StandardPrinter
{
    pub fn new() -> StandardPrinter
    {
        return StandardPrinter{};
    }
}

impl Printer for StandardPrinter
{
    fn print_single_banner_line(
        &mut self, banner_text : &str, banner_color : Color, path : &str)
    {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        match stdout.set_color(ColorSpec::new().set_fg(Some(banner_color)))
        {
            Ok(_) => {},
            Err(_error) => {},
        }
        match write!(&mut stdout, "{}: ", banner_text)
        {
            Ok(_) => {},
            Err(_error) =>
            {
                /*  If the write doesn't work, change the color back, but
                    other than that, I don't know what to do. */
                match stdout.set_color(ColorSpec::new().set_fg(None))
                {
                    Ok(_) => {},
                    Err(_error) => {},
                }
                return
            }
        }
        match stdout.set_color(ColorSpec::new().set_fg(None))
        {
            Ok(_) => {},
            Err(_error) => {},
        }
        match writeln!(&mut stdout, "{}", path)
        {
            Ok(_) => {},
            Err(_error) =>
            {
                // Again, just not sure what to do if write fails.
            },
        }
    }

    fn print(
        &mut self, text : &str)
    {
        println!("{}", text);
    }

    fn error(
        &mut self, text : &str)
    {
        println!("{}", text);
    }
}

#[cfg(test)]
pub struct EmptyPrinter
{
}

#[cfg(test)]
impl EmptyPrinter
{
    pub fn new() -> EmptyPrinter
    {
        return EmptyPrinter{}
    }
}

#[cfg(test)]
impl Printer for EmptyPrinter
{
    fn print_single_banner_line(
        &mut self, _banner_text : &str, _banner_color : Color, _path : &str)
    {
    }

    fn print(
        &mut self, _text : &str)
    {
    }

    fn error(
        &mut self, _text: &str)
    {
    }
}
