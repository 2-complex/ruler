use crate::system::
{
    System,
    SystemError,
};

use std::io::Write;
use std::cmp::max;

/*  For accumulating pieces of data (a series of Vec<u8>'s) which might arrive
    out-of-order.  Caller uses the function create_file to create a file, and
    at the same time, a PiecemealFileWriter.  When new data arrives, give the
    new data to the PiecemealFileWriter using obtain(), when the total number
    of pieces is known, use limit() to inform the writer of the total.

    Once all the pieces have been delivered to the file, a call to obtain() or
    limit returns Done. */
pub struct PiecemealFileWriter
{
    pieces : Vec<Option<Vec<u8>>>,
    next_index : usize,
    end_index_opt : Option<usize>,
}

pub enum PiecemealFileWriterResult
{
    Continue,
    Done,
    Contradiction,
    IOError(std::io::Error),
}

impl PiecemealFileWriter
{
    pub fn create_file
    <
        SystemType : System
    >
    (
        system : &mut SystemType,
        path_str : &str
    )
    ->
    Result<(PiecemealFileWriter, SystemType::File), SystemError>
    {
        match system.create_file(path_str)
        {
            Ok(file) => Ok(
                (
                    PiecemealFileWriter
                    {
                        pieces : vec![],
                        next_index : 0,
                        end_index_opt : None,
                    },
                    file
                )
            ),

            Err(error) => Err(error)
        }
    }

    /*  Use this function to provide a piece of the file with an index.
        This function then goes on to write as many continguous pieces
        to the file object as it can.  If it finishes the file doing this
        returns Done. */
    pub fn obtain
    <
        FileType : Write
    >
    (
        &mut self,
        buffer : Vec<u8>,
        index : usize,
        file : &mut FileType
    )
    ->
    PiecemealFileWriterResult
    {
        match self.end_index_opt
        {
            Some(end_index) =>
            {
                if index >= end_index
                {
                    return PiecemealFileWriterResult::Contradiction;
                }
            },
            None => {},
        }

        self.pieces.resize(max(index + 1, self.pieces.len()), None);

        match self.pieces[index]
        {
            Some(_) => return PiecemealFileWriterResult::Contradiction,
            None => {},
        }

        self.pieces[index].replace(buffer);

        while self.next_index < self.pieces.len()
        {
            match self.pieces[self.next_index].take()
            {
                Some(buffer) =>
                {
                    self.next_index += 1;
                    match file.write_all(&buffer)
                    {
                        Ok(_) => {},
                        Err(error) => return PiecemealFileWriterResult::IOError(error),
                    }
                },
                None => break,
            }

            match self.end_index_opt
            {
                Some(end_index) =>
                {
                    if self.next_index == end_index
                    {
                        return PiecemealFileWriterResult::Done;
                    }
                    if self.next_index > end_index
                    {
                        return PiecemealFileWriterResult::Contradiction;
                    }
                },
                None => {},
            }
        }

        PiecemealFileWriterResult::Continue
    }

    /*  Establish the number of pieces that make up the file. */
    pub fn limit
    (
        &mut self,
        end_index : usize
    )
    ->
    PiecemealFileWriterResult
    {
        self.end_index_opt.replace(end_index);
        if self.next_index < end_index
        {
            return PiecemealFileWriterResult::Continue;
        }
        else if self.next_index == end_index
        {
            return PiecemealFileWriterResult::Done;
        }
        else
        {
            return PiecemealFileWriterResult::Contradiction;
        }
    }
}


#[cfg(test)]
mod test
{
    use crate::system::fake::FakeSystem;
    use crate::system::util::read_file_to_string;
    use crate::piecemeal::
    {
        PiecemealFileWriter,
        PiecemealFileWriterResult,
    };

    /*  Create a PiecemealFileWriter and set the limit to 0.  Check that it
        produces an empty file. */
    #[test]
    fn zero_pieces_empty_file()
    {
        let mut system = FakeSystem::new(120);

        let (mut writer, _file) = match PiecemealFileWriter::create_file(&mut system, "out")
        {
            Ok((writer, _file)) => (writer, _file),
            Err(error) => panic!("File failed to create: {}", error),
        };

        match writer.limit(0usize)
        {
            PiecemealFileWriterResult::Done => {},
            _ => panic!("Limit 0 wrong return type"),
        }

        match read_file_to_string(&mut system, "out")
        {
            Ok(content) => assert_eq!(content, ""),
            Err(_) => panic!("Ouptut file was not there"),
        }
    }

    /*  Create a PiecemealFileWriter, send it 1 piece whose contents is known
        text.  Set the limit as 1, check that it writes that text to the
        file. */
    #[test]
    fn one_piece_file_matches()
    {
        let mut system = FakeSystem::new(120);

        let (mut writer, mut file) = match PiecemealFileWriter::create_file(&mut system, "out")
        {
            Ok((writer, file)) => (writer, file),
            Err(error) => panic!("File failed to create: {}", error),
        };

        match writer.limit(1usize)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After setting limit to 1, wrong return"),
        }

        match writer.obtain("jeff".as_bytes().to_vec(), 0, &mut file)
        {
            PiecemealFileWriterResult::Done => {},
            _ => panic!("After obtaining the last piece, wrong return"),
        }

        match read_file_to_string(&mut system, "out")
        {
            Ok(content) => assert_eq!(content, "jeff"),
            Err(_) => panic!("Ouptut file was not there"),
        }
    }

    /*  Create a PiecemealFileWriter.  Set the limit as 1.  Then send it 1
        piece whose contents is known text.  Check that it writes that text to
        the file. */
    #[test]
    fn one_piece_file_matches_other_way_around()
    {
        let mut system = FakeSystem::new(120);

        let (mut writer, mut file) = match PiecemealFileWriter::create_file(&mut system, "out")
        {
            Ok((writer, file)) => (writer, file),
            Err(error) => panic!("File failed to create: {}", error),
        };

        match writer.obtain("jeff".as_bytes().to_vec(), 0, &mut file)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After obtaining the last piece, wrong return"),
        }
        match writer.limit(1usize)
        {
            PiecemealFileWriterResult::Done => {},
            _ => panic!("After setting limit to 1, wrong return"),
        }

        match read_file_to_string(&mut system, "out")
        {
            Ok(content) => assert_eq!(content, "jeff"),
            Err(_) => panic!("Ouptut file was not there"),
        }
    }

    /*  Create a PiecemealFileWriter.  Set the limit to 2.  Then send it 2
        pieces whose contents is known text.  Check that it writes that text to
        the file. */
    #[test]
    fn two_pieces_file_matches()
    {
        let mut system = FakeSystem::new(120);

        let (mut writer, mut file) = match PiecemealFileWriter::create_file(&mut system, "out")
        {
            Ok((writer, file)) => (writer, file),
            Err(error) => panic!("File failed to create: {}", error),
        };

        match writer.limit(2usize)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After setting limit to 2, wrong return"),
        }

        match writer.obtain("apple\n".as_bytes().to_vec(), 0, &mut file)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After obtaining piece, wrong return"),
        }

        match writer.obtain("banana\n".as_bytes().to_vec(), 1, &mut file)
        {
            PiecemealFileWriterResult::Done => {},
            _ => panic!("After obtaining piece, wrong return"),
        }

        match read_file_to_string(&mut system, "out")
        {
            Ok(content) => assert_eq!(content, "apple\nbanana\n"),
            Err(_) => panic!("Ouptut file was not there"),
        }
    }

    /*  Create a PiecemealFileWriter.  Set the limit to 2.  Then send it 2
        pieces whose contents is known text, but send piece 1 first then
        piece 0.  Check that it writes that text to the file. */
    #[test]
    fn two_pieces_file_matches_reverse()
    {
        let mut system = FakeSystem::new(120);

        let (mut writer, mut file) = match PiecemealFileWriter::create_file(&mut system, "out")
        {
            Ok((writer, file)) => (writer, file),
            Err(error) => panic!("File failed to create: {}", error),
        };

        match writer.limit(2usize)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After setting limit to 2, wrong return"),
        }

        match writer.obtain("banana\n".as_bytes().to_vec(), 1, &mut file)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After obtaining piece, wrong return"),
        }

        match writer.obtain("apple\n".as_bytes().to_vec(), 0, &mut file)
        {
            PiecemealFileWriterResult::Done => {},
            _ => panic!("After obtaining piece, wrong return"),
        }

        match read_file_to_string(&mut system, "out")
        {
            Ok(content) => assert_eq!(content, "apple\nbanana\n"),
            Err(_) => panic!("Ouptut file was not there"),
        }
    }

    /*  Create a PiecemealFileWriter.  Then send it 2 pieces whose contents is
        known text.  Set the limit to 2 in between the two pieces. Check that
        it writes that text to the file. */
    #[test]
    fn two_pieces_file_matches_limit_in_between()
    {
        let mut system = FakeSystem::new(120);

        let (mut writer, mut file) = match PiecemealFileWriter::create_file(&mut system, "out")
        {
            Ok((writer, file)) => (writer, file),
            Err(error) => panic!("File failed to create: {}", error),
        };

        match writer.obtain("apple\n".as_bytes().to_vec(), 0, &mut file)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After obtaining piece, wrong return"),
        }

        match writer.limit(2usize)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After setting limit to 2, wrong return"),
        }

        match writer.obtain("banana\n".as_bytes().to_vec(), 1, &mut file)
        {
            PiecemealFileWriterResult::Done => {},
            _ => panic!("After obtaining piece, wrong return"),
        }

        match read_file_to_string(&mut system, "out")
        {
            Ok(content) => assert_eq!(content, "apple\nbanana\n"),
            Err(_) => panic!("Ouptut file was not there"),
        }
    }

    /*  Create a PiecemealFileWriter.  Then send it 2 pieces, but a limit of 1.
        Check that returns Contradiction. */
    #[test]
    fn two_pieces_but_limit_to_one()
    {
        let mut system = FakeSystem::new(120);

        let (mut writer, mut file) = match PiecemealFileWriter::create_file(&mut system, "out")
        {
            Ok((writer, file)) => (writer, file),
            Err(error) => panic!("File failed to create: {}", error),
        };

        match writer.obtain("apple\n".as_bytes().to_vec(), 0, &mut file)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After obtaining piece, wrong return"),
        }

        match writer.limit(1usize)
        {
            PiecemealFileWriterResult::Done => {},
            _ => panic!("After setting limit to 2, wrong return"),
        }

        match writer.obtain("banana\n".as_bytes().to_vec(), 1, &mut file)
        {
            PiecemealFileWriterResult::Contradiction => {},
            _ => panic!("After obtaining piece, wrong return"),
        }
    }

    /*  Create a PiecemealFileWriter.  Then send it 2 pieces, and _then_ a
        limit of 1.  Check that returns Contradiction. */
    #[test]
    fn two_pieces_file_matches_and_then_limit_to_one()
    {
        let mut system = FakeSystem::new(120);

        let (mut writer, mut file) = match PiecemealFileWriter::create_file(&mut system, "out")
        {
            Ok((writer, file)) => (writer, file),
            Err(error) => panic!("File failed to create: {}", error),
        };

        match writer.obtain("apple\n".as_bytes().to_vec(), 0, &mut file)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After obtaining piece, wrong return"),
        }

        match writer.obtain("banana\n".as_bytes().to_vec(), 1, &mut file)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After obtaining piece, wrong return"),
        }

        match writer.limit(1usize)
        {
            PiecemealFileWriterResult::Contradiction => {},
            _ => panic!("After setting limit to 2, wrong return"),
        }
    }

    /*  Create a PiecemealFileWriter.  Then send it 2 pieces, but send the
        second one to the same index.  Check that this returns
        Contradiction. */
    #[test]
    fn two_pieces_in_one_position()
    {
        let mut system = FakeSystem::new(120);

        let (mut writer, mut file) = match PiecemealFileWriter::create_file(&mut system, "out")
        {
            Ok((writer, file)) => (writer, file),
            Err(error) => panic!("File failed to create: {}", error),
        };

        match writer.obtain("apple\n".as_bytes().to_vec(), 1, &mut file)
        {
            PiecemealFileWriterResult::Continue => {},
            _ => panic!("After obtaining piece, wrong return"),
        }

        match writer.obtain("banana\n".as_bytes().to_vec(), 1, &mut file)
        {
            PiecemealFileWriterResult::Contradiction => {},
            _ => panic!("After obtaining piece, wrong return"),
        }
    }
}
